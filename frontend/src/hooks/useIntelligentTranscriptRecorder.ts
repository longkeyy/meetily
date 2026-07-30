import { useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { meetingIntelligenceService } from '@/services/meetingIntelligenceService';

const UPDATE_INTERVAL_MS = 150_000;

export function useIntelligentTranscriptRecorder() {
  const { isRecording, isPaused } = useRecordingState();
  const { transcriptsRef } = useTranscripts();
  const inFlightRef = useRef(false);
  const lastRevisionRef = useRef(0);

  useEffect(() => {
    if (!isRecording || isPaused) return;
    let disposed = false;

    const updateDetailedRecord = async () => {
      if (inFlightRef.current || disposed) return;
      const transcripts = [...transcriptsRef.current];
      const revision = transcripts.reduce(
        (latest, transcript) => Math.max(latest, transcript.sequence_id ?? 0),
        0,
      );
      if (revision <= lastRevisionRef.current || transcripts.length === 0) return;

      inFlightRef.current = true;
      try {
        const settings = await meetingIntelligenceService.getSettings();
        if (!settings.intelligentTranscriptEnabled) return;
        const meetingFolder = await invoke<string>('get_meeting_folder_path');
        await meetingIntelligenceService.generateLive(meetingFolder, transcripts);
        lastRevisionRef.current = revision;
      } catch (error) {
        console.warn('Failed to update the intelligent transcript:', error);
      } finally {
        inFlightRef.current = false;
      }
    };

    const timer = window.setInterval(() => void updateDetailedRecord(), UPDATE_INTERVAL_MS);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [isPaused, isRecording, transcriptsRef]);

  useEffect(() => {
    if (isRecording) {
      lastRevisionRef.current = 0;
    }
  }, [isRecording]);
}
