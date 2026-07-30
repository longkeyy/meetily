import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { meetingIntelligenceService } from '@/services/meetingIntelligenceService';
import type { IntelligentTranscriptDocument } from '@/types/meeting-intelligence';
import { completedTurnRevision } from '@/lib/refined-transcript';

export type RefinedTranscriptStatus = 'idle' | 'waiting' | 'generating' | 'ready' | 'error';

export interface RefinedTranscriptState {
  document: IntelligentTranscriptDocument | null;
  status: RefinedTranscriptStatus;
  error: string | null;
}

export function useIntelligentTranscriptRecorder(): RefinedTranscriptState {
  const { isRecording, isPaused } = useRecordingState();
  const { transcripts } = useTranscripts();
  const queueRef = useRef<Promise<void>>(Promise.resolve());
  const lastQueuedRevisionRef = useRef(0);
  const pendingCountRef = useRef(0);
  const sessionRef = useRef(0);
  const [state, setState] = useState<RefinedTranscriptState>({
    document: null,
    status: 'idle',
    error: null,
  });

  useEffect(() => {
    if (isRecording) {
      sessionRef.current += 1;
      queueRef.current = Promise.resolve();
      lastQueuedRevisionRef.current = 0;
      pendingCountRef.current = 0;
      setState({ document: null, status: 'waiting', error: null });
    }
  }, [isRecording]);

  useEffect(() => {
    if (!isRecording || isPaused) return;
    const revision = completedTurnRevision(transcripts);
    if (revision <= lastQueuedRevisionRef.current) return;

    lastQueuedRevisionRef.current = revision;
    const snapshot = [...transcripts];
    const session = sessionRef.current;
    pendingCountRef.current += 1;
    setState((current) => ({ ...current, status: 'generating', error: null }));
    queueRef.current = queueRef.current
      .catch(() => undefined)
      .then(async () => {
        let disabled = false;
        try {
          const settings = await meetingIntelligenceService.getSettings();
          if (!settings.intelligentTranscriptEnabled) {
            disabled = true;
            return;
          }
          const meetingFolder = await invoke<string>('get_meeting_folder_path');
          const response = await meetingIntelligenceService.generateLive(meetingFolder, snapshot);
          if (session === sessionRef.current) {
            setState((current) => ({ ...current, document: response.document, error: null }));
          }
        } catch (error) {
          console.warn('Failed to refine completed transcript turn:', error);
          if (session === sessionRef.current) {
            setState((current) => ({ ...current, error: String(error) }));
          }
        } finally {
          if (session !== sessionRef.current) return;
          pendingCountRef.current = Math.max(0, pendingCountRef.current - 1);
          setState((current) => ({
            ...current,
            status: disabled
              ? 'idle'
              : pendingCountRef.current > 0
                ? 'generating'
                : current.error
                  ? 'error'
                  : current.document
                    ? 'ready'
                    : 'waiting',
          }));
        }
      });
  }, [isPaused, isRecording, transcripts]);

  return state;
}
