import { invoke } from '@tauri-apps/api/core';
import { Transcript } from '@/types';
import {
  GenerateIntelligentTranscriptRequest,
  IntelligentTranscriptDocument,
  IntelligentTranscriptResponse,
  MeetingIntelligenceSettings,
  MeetingIntelligenceSettingsUpdate,
  RealtimeSummaryDocument,
  RealtimeSummaryResponse,
} from '@/types/meeting-intelligence';

function toRequestTranscripts(transcripts: Transcript[]): GenerateIntelligentTranscriptRequest['transcripts'] {
  return transcripts
    .filter((transcript) => transcript.text.trim() && transcript.is_partial !== true)
    .map((transcript) => ({
      sequenceId: transcript.sequence_id,
      source: transcript.source,
      text: transcript.text,
      audioStartTime: transcript.audio_start_time,
      audioEndTime: transcript.audio_end_time,
    }));
}

export const meetingIntelligenceService = {
  getSettings(): Promise<MeetingIntelligenceSettings> {
    return invoke('api_get_meeting_intelligence_settings');
  },

  saveSettings(settingsUpdate: MeetingIntelligenceSettingsUpdate): Promise<MeetingIntelligenceSettings> {
    return invoke('api_save_meeting_intelligence_settings', { settingsUpdate });
  },

  async generateLive(
    meetingFolder: string,
    transcripts: Transcript[],
    forceFull = false,
  ): Promise<IntelligentTranscriptResponse> {
    const request: GenerateIntelligentTranscriptRequest = {
      requestId: `${Date.now()}-${crypto.randomUUID()}`,
      meetingFolder,
      transcripts: toRequestTranscripts(transcripts),
      forceFull,
    };
    return invoke('api_generate_intelligent_transcript', { request });
  },

  getForMeeting(meetingId: string): Promise<IntelligentTranscriptDocument | null> {
    return invoke('api_get_intelligent_transcript', { meetingId });
  },

  regenerateForMeeting(meetingId: string): Promise<IntelligentTranscriptDocument> {
    return invoke('api_regenerate_intelligent_transcript', { meetingId });
  },

  async generateRealtime(
    meetingFolder: string,
    transcripts: Transcript[],
    forceFull = false,
  ): Promise<RealtimeSummaryResponse> {
    const request: GenerateIntelligentTranscriptRequest = {
      requestId: `${Date.now()}-${crypto.randomUUID()}`,
      meetingFolder,
      transcripts: toRequestTranscripts(transcripts),
      forceFull,
    };
    return invoke('api_generate_realtime_summary', { request });
  },

  getRealtimeForMeeting(meetingId: string): Promise<RealtimeSummaryDocument | null> {
    return invoke('api_get_realtime_summary', { meetingId });
  },

  regenerateRealtimeForMeeting(meetingId: string): Promise<RealtimeSummaryDocument> {
    return invoke('api_regenerate_realtime_summary', { meetingId });
  },
};
