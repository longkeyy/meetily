import { useCallback, useEffect, useReducer, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { Transcript, SourceActivityEvent, AssistantSuggestionRequest, AssistantSuggestionResponse } from '@/types';
import {
  assistantReducer,
  AssistantState,
  initialAssistantState,
  SuggestionTrigger,
} from '@/lib/conversation-assistant';

const ENABLED_STORAGE_KEY = 'conversationAssistant.interview.enabled';
const PERIODIC_INTERVAL_MS = 30_000;
const TURN_END_GRACE_MS = 800;
const FINAL_TRANSCRIPT_WAIT_MS = 10_000;
const TRANSCRIPT_COVERAGE_TOLERANCE_SECONDS = 1.5;
const HISTORY_WINDOW_SECONDS = 10 * 60;
const MAX_TRANSCRIPTS = 200;

interface PendingTrigger {
  trigger: SuggestionTrigger;
  focusStartTime: number;
  targetEndTime: number;
}

interface UseConversationAssistantOptions {
  isRecording: boolean;
  isPaused: boolean;
  transcripts: Transcript[];
}

function readInitialEnabled(): boolean {
  if (typeof window === 'undefined') return false;
  return localStorage.getItem(ENABLED_STORAGE_KEY) === 'true';
}

export function useConversationAssistant({
  isRecording,
  isPaused,
  transcripts,
}: UseConversationAssistantOptions) {
  const initialEnabled = readInitialEnabled();
  const [state, dispatch] = useReducer(assistantReducer, {
    ...initialAssistantState,
    enabled: initialEnabled,
    status: initialEnabled ? 'waiting' : 'disabled',
  } satisfies AssistantState);

  const stateRef = useRef(state);
  const transcriptsRef = useRef(transcripts);
  const speakerActiveRef = useRef(false);
  const micActiveRef = useRef(false);
  const speakerTurnStartRef = useRef<number | null>(null);
  const requestCounterRef = useRef(0);
  const activeRequestIdRef = useRef<string | null>(null);
  const pendingTriggerRef = useRef<PendingTrigger | null>(null);
  const periodicTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const turnEndTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const finalWaitTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    stateRef.current = state;
  }, [state]);

  useEffect(() => {
    transcriptsRef.current = transcripts;
  }, [transcripts]);

  const clearTimer = useCallback((timerRef: React.MutableRefObject<ReturnType<typeof setTimeout> | null>) => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const cancelGeneration = useCallback(() => {
    activeRequestIdRef.current = null;
    dispatch({ type: 'generationCancelled' });
    void invoke<boolean>('api_cancel_assistant_suggestion').catch(() => false);
  }, []);

  const clearPendingWork = useCallback(() => {
    pendingTriggerRef.current = null;
    clearTimer(periodicTimerRef);
    clearTimer(turnEndTimerRef);
    clearTimer(finalWaitTimerRef);
  }, [clearTimer]);

  const assistantTranscripts = useCallback(() => {
    const usable = transcriptsRef.current.filter(
      (transcript): transcript is Transcript & {
        source: 'mic' | 'system';
        audio_start_time: number;
        audio_end_time: number;
      } => Boolean(
        transcript.source
        && transcript.text.trim()
        && transcript.audio_start_time !== undefined
        && transcript.audio_end_time !== undefined,
      ),
    );
    const latestEnd = usable.at(-1)?.audio_end_time ?? 0;
    return usable
      .filter((transcript) => transcript.audio_end_time >= latestEnd - HISTORY_WINDOW_SECONDS)
      .slice(-MAX_TRANSCRIPTS)
      .map((transcript) => ({
        source: transcript.source,
        text: transcript.text,
        audioStartTime: transcript.audio_start_time,
        audioEndTime: transcript.audio_end_time,
      }));
  }, []);

  const generateSuggestion = useCallback(async (
    trigger: SuggestionTrigger,
    focusStartTime: number,
  ) => {
    if (!stateRef.current.enabled || micActiveRef.current || !isRecording || isPaused) return;

    const context = assistantTranscripts();
    if (!context.some((transcript) => transcript.source === 'system' && transcript.audioEndTime >= focusStartTime)) {
      return;
    }

    const requestId = `${Date.now()}-${++requestCounterRef.current}`;
    const request: AssistantSuggestionRequest = {
      requestId,
      profile: 'interview',
      trigger,
      focusStartTime,
      transcripts: context,
    };
    activeRequestIdRef.current = requestId;
    dispatch({ type: 'generationStarted', requestId });

    try {
      const response = await invoke<AssistantSuggestionResponse>(
        'api_generate_assistant_suggestion',
        { request },
      );
      if (activeRequestIdRef.current !== response.requestId) return;
      activeRequestIdRef.current = null;
      dispatch({
        type: 'generationSucceeded',
        requestId: response.requestId,
        suggestion: response.suggestion,
      });
    } catch (error) {
      if (activeRequestIdRef.current !== requestId) return;
      activeRequestIdRef.current = null;
      dispatch({
        type: 'generationFailed',
        requestId,
        error: String(error),
      });
    }
  }, [assistantTranscripts, isPaused, isRecording]);

  const tryRunPendingTrigger = useCallback((allowIncomplete = false) => {
    const pending = pendingTriggerRef.current;
    if (!pending || micActiveRef.current || !stateRef.current.enabled) return;

    const latestSystemEnd = transcriptsRef.current.reduce((latest, transcript) => {
      if (transcript.source !== 'system' || transcript.audio_end_time === undefined) return latest;
      return Math.max(latest, transcript.audio_end_time);
    }, 0);
    const hasCoverage = latestSystemEnd >= pending.targetEndTime - TRANSCRIPT_COVERAGE_TOLERANCE_SECONDS;
    if (!hasCoverage && !allowIncomplete) return;
    if (latestSystemEnd < pending.focusStartTime) return;

    pendingTriggerRef.current = null;
    clearTimer(finalWaitTimerRef);
    void generateSuggestion(pending.trigger, pending.focusStartTime);
  }, [clearTimer, generateSuggestion]);

  useEffect(() => {
    tryRunPendingTrigger();
  }, [transcripts, tryRunPendingTrigger]);

  const schedulePeriodicTrigger = useCallback((turnStartTime: number) => {
    clearTimer(periodicTimerRef);
    let targetEndTime = turnStartTime + PERIODIC_INTERVAL_MS / 1000;

    const scheduleNext = () => {
      periodicTimerRef.current = setTimeout(() => {
        periodicTimerRef.current = null;
        if (!speakerActiveRef.current || micActiveRef.current || !stateRef.current.enabled) return;
        pendingTriggerRef.current = {
          trigger: 'periodic',
          focusStartTime: targetEndTime - PERIODIC_INTERVAL_MS / 1000,
          targetEndTime,
        };
        tryRunPendingTrigger();
        targetEndTime += PERIODIC_INTERVAL_MS / 1000;
        scheduleNext();
      }, PERIODIC_INTERVAL_MS);
    };

    scheduleNext();
  }, [clearTimer, tryRunPendingTrigger]);

  const completeSpeakerTurn = useCallback((turnEndTime: number) => {
    clearTimer(periodicTimerRef);
    if (micActiveRef.current || !stateRef.current.enabled) {
      speakerTurnStartRef.current = null;
      pendingTriggerRef.current = null;
      return;
    }

    const turnStartTime = speakerTurnStartRef.current ?? Math.max(0, turnEndTime - 30);
    speakerTurnStartRef.current = null;
    pendingTriggerRef.current = {
      trigger: 'turnEnd',
      focusStartTime: turnStartTime,
      targetEndTime: turnEndTime,
    };
    tryRunPendingTrigger();
    clearTimer(finalWaitTimerRef);
    finalWaitTimerRef.current = setTimeout(
      () => tryRunPendingTrigger(true),
      FINAL_TRANSCRIPT_WAIT_MS,
    );
  }, [clearTimer, tryRunPendingTrigger]);

  useEffect(() => {
    if (!isRecording) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;

    void listen<SourceActivityEvent>('audio-source-activity', (event) => {
      const activity = event.payload;
      if (activity.source === 'mic') {
        micActiveRef.current = activity.active;
        dispatch({ type: 'sourceActivity', source: 'mic', active: activity.active });
        if (activity.active) {
          clearPendingWork();
          cancelGeneration();
        } else if (speakerActiveRef.current && stateRef.current.enabled) {
          speakerTurnStartRef.current = activity.timestamp;
          schedulePeriodicTrigger(activity.timestamp);
        }
        return;
      }

      speakerActiveRef.current = activity.active;
      dispatch({ type: 'sourceActivity', source: 'system', active: activity.active });
      if (activity.active) {
        clearTimer(turnEndTimerRef);
        pendingTriggerRef.current = null;
        const isNewTurn = speakerTurnStartRef.current === null;
        if (isNewTurn) {
          speakerTurnStartRef.current = activity.timestamp;
        }
        if (stateRef.current.enabled && !micActiveRef.current) {
          if (isNewTurn) cancelGeneration();
          if (periodicTimerRef.current === null) {
            schedulePeriodicTrigger(speakerTurnStartRef.current ?? activity.timestamp);
          }
        }
      } else {
        clearTimer(turnEndTimerRef);
        turnEndTimerRef.current = setTimeout(
          () => completeSpeakerTurn(activity.timestamp),
          TURN_END_GRACE_MS,
        );
      }
    }).then((dispose) => {
      if (disposed) {
        void dispose();
      } else {
        unlisten = dispose;
      }
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [
    cancelGeneration,
    clearPendingWork,
    clearTimer,
    completeSpeakerTurn,
    isRecording,
    schedulePeriodicTrigger,
  ]);

  useEffect(() => {
    if (isRecording && !isPaused) return;
    clearPendingWork();
    cancelGeneration();
    if (!isRecording) {
      speakerActiveRef.current = false;
      micActiveRef.current = false;
      speakerTurnStartRef.current = null;
      dispatch({ type: 'reset' });
    }
  }, [cancelGeneration, clearPendingWork, isPaused, isRecording]);

  useEffect(() => () => {
    clearPendingWork();
    void invoke<boolean>('api_cancel_assistant_suggestion').catch(() => false);
  }, [clearPendingWork]);

  const setEnabled = useCallback((enabled: boolean) => {
    localStorage.setItem(ENABLED_STORAGE_KEY, String(enabled));
    dispatch({ type: 'setEnabled', enabled });
    if (!enabled) {
      clearPendingWork();
      cancelGeneration();
      return;
    }
    if (speakerActiveRef.current && !micActiveRef.current) {
      const latestTime = transcriptsRef.current.at(-1)?.audio_end_time ?? 0;
      speakerTurnStartRef.current ??= latestTime;
      schedulePeriodicTrigger(speakerTurnStartRef.current);
    }
  }, [cancelGeneration, clearPendingWork, schedulePeriodicTrigger]);

  const refreshSuggestion = useCallback(() => {
    const latestTime = transcriptsRef.current.reduce(
      (latest, transcript) => Math.max(latest, transcript.audio_end_time ?? 0),
      0,
    );
    void generateSuggestion('manual', Math.max(0, latestTime - 30));
  }, [generateSuggestion]);

  return {
    state,
    setEnabled,
    refreshSuggestion,
  };
}
