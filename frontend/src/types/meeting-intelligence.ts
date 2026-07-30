import { Transcript } from '@/types';

export interface MeetingIntelligenceSettings {
  intelligentTranscriptEnabled: boolean;
  intelligentTranscriptPrompt: string;
  defaultIntelligentTranscriptPrompt: string;
}

export interface MeetingIntelligenceSettingsUpdate {
  intelligentTranscriptEnabled: boolean;
  intelligentTranscriptPrompt: string;
}

export interface IntelligentTranscriptDocument {
  version: number;
  markdown: string;
  coveredUntil: number;
  sourceRevision: number;
  updatedAt: string;
}

export interface IntelligentTranscriptResponse {
  requestId: string;
  document: IntelligentTranscriptDocument;
}

export interface GenerateIntelligentTranscriptRequest {
  requestId: string;
  meetingFolder: string;
  transcripts: Array<{
    sequenceId?: number;
    source?: Transcript['source'];
    text: string;
    audioStartTime?: number;
    audioEndTime?: number;
  }>;
  forceFull: boolean;
}
