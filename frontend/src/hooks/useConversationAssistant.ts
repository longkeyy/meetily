import { useCallback, useEffect, useReducer, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { Transcript, SourceActivityEvent, AssistantSuggestionRequest, AssistantSuggestionResponse } from '@/types';
import {
  assistantReducer,
  AssistantTriggerCheckpoint,
  AssistantState,
  enqueueSuggestionTrigger,
  initialAssistantState,
  periodicSuggestionTrigger,
  SuggestionTrigger,
  takeReadySuggestionTrigger,
  turnEndSuggestionTrigger,
} from '@/lib/conversation-assistant';
import { assistantSettingsService } from '@/services/assistantSettingsService';
import {
  AssistantSettings,
  AssistantSettingsUpdate,
  FALLBACK_ASSISTANT_SETTINGS,
} from '@/types/assistant-settings';

const ENABLED_STORAGE_KEY = 'conversationAssistant.interview.enabled';
const TURN_END_GRACE_MS = 800;
const FINAL_TRANSCRIPT_WAIT_MS = 10_000;
const TRANSCRIPT_COVERAGE_TOLERANCE_SECONDS = 1.5;
const HISTORY_WINDOW_SECONDS = 10 * 60;
const MAX_TRANSCRIPTS = 200;

interface UseConversationAssistantOptions {
  isRecording: boolean;
  isPaused: boolean;
  transcripts: Transcript[];
}

function toSettingsUpdate(settings: AssistantSettings): AssistantSettingsUpdate {
  return {
    enabledByDefault: settings.enabledByDefault,
    profile: settings.profile,
    intervalSeconds: settings.intervalSeconds,
    modelMode: settings.modelMode,
    provider: settings.modelMode === 'custom' ? settings.provider : null,
    model: settings.modelMode === 'custom' ? settings.model : null,
    customOpenAIBaseUrl: settings.modelMode === 'custom' ? settings.customOpenAIBaseUrl : null,
    customOpenAIApiKey: settings.modelMode === 'custom' ? settings.customOpenAIApiKey : null,
    systemPrompt: settings.systemPrompt,
  };
}

export function useConversationAssistant({
  isRecording,
  isPaused,
  transcripts,
}: UseConversationAssistantOptions) {
  const [state, dispatch] = useReducer(assistantReducer, initialAssistantState);
  const [assistantSettings, setAssistantSettings] = useState(FALLBACK_ASSISTANT_SETTINGS);
  const [settingsReady, setSettingsReady] = useState(false);

  const stateRef = useRef(state);
  const settingsRef = useRef(FALLBACK_ASSISTANT_SETTINGS);
  const transcriptsRef = useRef(transcripts);
  const speakerActiveRef = useRef(false);
  const micActiveRef = useRef(false);
  const speakerTurnStartRef = useRef<number | null>(null);
  const requestCounterRef = useRef(0);
  const activeRequestIdRef = useRef<string | null>(null);
  const pendingTriggersRef = useRef<AssistantTriggerCheckpoint[]>([]);
  const periodicTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const turnEndTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const finalWaitTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    stateRef.current = state;
  }, [state]);

  useEffect(() => {
    transcriptsRef.current = transcripts;
  }, [transcripts]);

  useEffect(() => {
    let disposed = false;
    void assistantSettingsService.get().then(async (loaded) => {
      let resolved = loaded;
      const legacyEnabled = localStorage.getItem(ENABLED_STORAGE_KEY);
      if (!loaded.isConfigured && legacyEnabled !== null) {
        resolved = await assistantSettingsService.save({
          ...toSettingsUpdate(loaded),
          enabledByDefault: legacyEnabled === 'true',
        });
      }
      localStorage.removeItem(ENABLED_STORAGE_KEY);
      if (disposed) return;
      settingsRef.current = resolved;
      setAssistantSettings(resolved);
      setSettingsReady(true);
      dispatch({
        type: 'settingsLoaded',
        enabled: resolved.enabledByDefault,
        profile: resolved.profile,
      });
    }).catch((error) => {
      console.error('Failed to load assistant settings:', error);
    });
    return () => {
      disposed = true;
    };
  }, []);

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
    pendingTriggersRef.current = [];
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
      profile: settingsRef.current.profile,
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
    if (pendingTriggersRef.current.length === 0 || micActiveRef.current || !stateRef.current.enabled) return;

    const latestSystemEnd = transcriptsRef.current.reduce((latest, transcript) => {
      if (transcript.source !== 'system' || transcript.audio_end_time === undefined) return latest;
      return Math.max(latest, transcript.audio_end_time);
    }, 0);
    const ready = takeReadySuggestionTrigger(
      pendingTriggersRef.current,
      latestSystemEnd,
      TRANSCRIPT_COVERAGE_TOLERANCE_SECONDS,
      allowIncomplete,
    );
    if (!ready) return;

    pendingTriggersRef.current = ready.remaining;
    clearTimer(finalWaitTimerRef);
    void generateSuggestion(ready.trigger.trigger, ready.trigger.focusStartTime);
  }, [clearTimer, generateSuggestion]);

  useEffect(() => {
    tryRunPendingTrigger();
  }, [transcripts, tryRunPendingTrigger]);

  const schedulePeriodicTrigger = useCallback((turnStartTime: number) => {
    clearTimer(periodicTimerRef);
    const intervalSeconds = settingsRef.current.intervalSeconds;
    let targetEndTime = turnStartTime + intervalSeconds;

    const scheduleNext = () => {
      periodicTimerRef.current = setTimeout(() => {
        periodicTimerRef.current = null;
        if (!speakerActiveRef.current || micActiveRef.current || !stateRef.current.enabled) return;
        pendingTriggersRef.current = enqueueSuggestionTrigger(
          pendingTriggersRef.current,
          periodicSuggestionTrigger(turnStartTime, targetEndTime),
        );
        tryRunPendingTrigger();
        targetEndTime += intervalSeconds;
        scheduleNext();
      }, intervalSeconds * 1_000);
    };

    scheduleNext();
  }, [clearTimer, tryRunPendingTrigger]);

  const completeSpeakerTurn = useCallback((turnEndTime: number) => {
    clearTimer(periodicTimerRef);
    if (micActiveRef.current || !stateRef.current.enabled) {
      speakerTurnStartRef.current = null;
      pendingTriggersRef.current = [];
      return;
    }

    const turnStartTime = speakerTurnStartRef.current
      ?? Math.max(0, turnEndTime - settingsRef.current.intervalSeconds);
    speakerTurnStartRef.current = null;
    pendingTriggersRef.current = enqueueSuggestionTrigger(
      pendingTriggersRef.current,
      turnEndSuggestionTrigger(turnStartTime, turnEndTime),
    );
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
    const updatedSettings = {
      ...settingsRef.current,
      enabledByDefault: enabled,
    };
    settingsRef.current = updatedSettings;
    setAssistantSettings(updatedSettings);
    dispatch({ type: 'setEnabled', enabled });
    void assistantSettingsService.save(toSettingsUpdate(updatedSettings))
      .then((saved) => {
        settingsRef.current = saved;
        setAssistantSettings(saved);
      })
      .catch((error) => console.error('Failed to persist assistant enabled state:', error));
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
    void generateSuggestion(
      'manual',
      Math.max(0, latestTime - settingsRef.current.intervalSeconds),
    );
  }, [generateSuggestion]);

  return {
    state,
    settings: assistantSettings,
    settingsReady,
    setEnabled,
    refreshSuggestion,
  };
}
