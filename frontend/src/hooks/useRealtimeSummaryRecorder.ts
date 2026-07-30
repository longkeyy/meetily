import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { meetingIntelligenceService } from '@/services/meetingIntelligenceService';
import { RealtimeSummaryDocument } from '@/types/meeting-intelligence';

export type RealtimeSummaryStatus = 'idle' | 'waiting' | 'generating' | 'ready' | 'error';

export function useRealtimeSummaryRecorder() {
  const { isRecording, isPaused } = useRecordingState();
  const { transcriptsRef } = useTranscripts();
  const [document, setDocument] = useState<RealtimeSummaryDocument | null>(null);
  const [status, setStatus] = useState<RealtimeSummaryStatus>('idle');
  const [error, setError] = useState<string | null>(null);
  const [enabled, setEnabled] = useState(true);
  const [intervalSeconds, setIntervalSeconds] = useState(120);
  const inFlightRef = useRef(false);
  const lastRevisionRef = useRef(0);

  const refresh = useCallback(async (manual = false) => {
    if (inFlightRef.current || !enabled) return;
    const transcripts = [...transcriptsRef.current];
    const revision = transcripts.reduce(
      (latest, transcript) => Math.max(latest, transcript.sequence_id ?? 0),
      0,
    );
    if (revision <= lastRevisionRef.current || transcripts.length === 0) return;
    if (transcripts.length === 0) return;

    inFlightRef.current = true;
    setStatus('generating');
    setError(null);
    try {
      const meetingFolder = await invoke<string>('get_meeting_folder_path');
      const response = await meetingIntelligenceService.generateRealtime(
        meetingFolder,
        transcripts,
        manual ? 'manual' : 'interval',
      );
      setDocument(response.document);
      lastRevisionRef.current = response.document.sourceRevision || revision;
      setStatus('ready');
    } catch (reason) {
      console.warn('Failed to update the realtime summary:', reason);
      setError(String(reason));
      setStatus('error');
    } finally {
      inFlightRef.current = false;
    }
  }, [enabled, transcriptsRef]);

  useEffect(() => {
    if (!isRecording) return;
    let disposed = false;
    void meetingIntelligenceService.getSettings()
      .then((settings) => {
        if (disposed) return;
        setEnabled(settings.realtimeSummaryEnabled);
        setIntervalSeconds(settings.realtimeSummaryIntervalSeconds);
        setStatus(settings.realtimeSummaryEnabled ? 'waiting' : 'idle');
      })
      .catch((reason) => {
        console.warn('Failed to load realtime summary settings:', reason);
        if (!disposed) {
          setEnabled(false);
          setError(String(reason));
          setStatus('error');
        }
      });
    return () => {
      disposed = true;
    };
  }, [isRecording]);

  useEffect(() => {
    if (!isRecording || isPaused) return;
    const timer = window.setInterval(
      () => void refresh(),
      intervalSeconds * 1000,
    );
    return () => window.clearInterval(timer);
  }, [intervalSeconds, isPaused, isRecording, refresh]);

  useEffect(() => {
    if (isRecording) {
      setDocument(null);
      setError(null);
      lastRevisionRef.current = 0;
      setStatus('waiting');
    } else {
      setStatus('idle');
    }
  }, [isRecording]);

  return { document, status, error, refresh, enabled };
}
