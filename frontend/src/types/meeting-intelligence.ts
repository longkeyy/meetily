import { Transcript } from '@/types';

export interface MeetingIntelligenceSettings {
  intelligentTranscriptEnabled: boolean;
  intelligentTranscriptPrompt: string;
  defaultIntelligentTranscriptPrompt: string;
  realtimeSummaryEnabled: boolean;
  realtimeSummaryIntervalSeconds: number;
  realtimeSummaryPrompt: string;
  defaultRealtimeSummaryPrompt: string;
}

export interface MeetingIntelligenceSettingsUpdate {
  intelligentTranscriptEnabled: boolean;
  intelligentTranscriptPrompt: string;
  realtimeSummaryEnabled: boolean;
  realtimeSummaryIntervalSeconds: number;
  realtimeSummaryPrompt: string;
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

export interface RealtimeSummaryDocument {
  version: number;
  markdown: string;
  coveredUntil: number;
  sourceRevision: number;
  updatedAt: string;
}

export interface RealtimeSummaryResponse {
  requestId: string;
  document: RealtimeSummaryDocument;
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

export type GenerateRealtimeSummaryRequest = GenerateIntelligentTranscriptRequest;
