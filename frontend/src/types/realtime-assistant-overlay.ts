import type { AssistantScheduleState } from '@/hooks/useConversationAssistant';
import type { AssistantState } from '@/lib/conversation-assistant';

export const REALTIME_ASSISTANT_WINDOW_LABEL = 'realtime-assistant';
export const REALTIME_ASSISTANT_STATE_EVENT = 'realtime-assistant-state';
export const REALTIME_ASSISTANT_ACTION_EVENT = 'realtime-assistant-action';

export interface RealtimeAssistantSnapshot {
  state: AssistantState;
  profileName: string;
  scheduleState: AssistantScheduleState;
  settingsReady: boolean;
  hasTranscripts: boolean;
}

export type RealtimeAssistantWindowAction =
  | { type: 'requestState' }
  | { type: 'setEnabled'; enabled: boolean }
  | { type: 'refresh' };
