import type { Transcript } from '@/types';
import type { ModelConfig } from '@/services/configService';

export type MeetingIntelligenceModelMode = 'followSummary' | 'custom';

export interface MeetingIntelligenceSettings {
  modelMode: MeetingIntelligenceModelMode;
  provider: ModelConfig['provider'] | null;
  model: string | null;
  apiKey: string | null;
  ollamaEndpoint: string | null;
  customOpenAIBaseUrl: string | null;
  customOpenAIApiKey: string | null;
  intelligentTranscriptEnabled: boolean;
  intelligentTranscriptPrompt: string;
  defaultIntelligentTranscriptPrompt: string;
  realtimeSummaryEnabled: boolean;
  realtimeSummaryIntervalSeconds: number;
  realtimeSummaryPrompt: string;
  defaultRealtimeSummaryPrompt: string;
}

export interface MeetingIntelligenceSettingsUpdate {
  modelMode: MeetingIntelligenceModelMode;
  provider: ModelConfig['provider'] | null;
  model: string | null;
  apiKey: string | null;
  ollamaEndpoint: string | null;
  customOpenAIBaseUrl: string | null;
  customOpenAIApiKey: string | null;
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
  segments: RealtimeSummarySegment[];
  coveredUntil: number;
  sourceRevision: number;
  updatedAt: string;
}

export type RealtimeSummaryTrigger = 'interval' | 'meetingEnd' | 'manual' | 'regenerate' | 'legacy';

export interface RealtimeSummarySegment {
  schemaVersion: number;
  segmentId: string;
  startSeconds: number;
  endSeconds: number;
  sourceRevisionStart: number;
  sourceRevisionEnd: number;
  contentFormat: 'markdown';
  content: string;
  trigger: RealtimeSummaryTrigger;
  createdAt: string;
  model: {
    provider: string;
    model: string;
  };
  promptHash: string;
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

export interface GenerateRealtimeSummaryRequest extends GenerateIntelligentTranscriptRequest {
  trigger?: RealtimeSummaryTrigger;
}

export function realtimeSummaryMarkdown(document: RealtimeSummaryDocument): string {
  return document.segments
    .map((segment) => `## ${formatSummaryTime(segment.startSeconds)} - ${formatSummaryTime(segment.endSeconds)}\n\n${segment.content}`)
    .join('\n\n');
}

export function formatSummaryTime(seconds: number): string {
  const safeSeconds = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(safeSeconds / 3600);
  const minutes = Math.floor((safeSeconds % 3600) / 60);
  const remaining = safeSeconds % 60;
  return hours > 0
    ? `${hours.toString().padStart(2, '0')}:${minutes.toString().padStart(2, '0')}:${remaining.toString().padStart(2, '0')}`
    : `${minutes.toString().padStart(2, '0')}:${remaining.toString().padStart(2, '0')}`;
}
