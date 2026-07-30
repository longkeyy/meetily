import type { AssistantScheduleState } from '@/hooks/useConversationAssistant';
import type { AssistantState } from '@/lib/conversation-assistant';

export const REALTIME_ASSISTANT_WINDOW_LABEL = 'realtime-assistant';
export const REALTIME_ASSISTANT_STATE_EVENT = 'realtime-assistant-state';
export const REALTIME_ASSISTANT_ACTION_EVENT = 'realtime-assistant-action';
export const REALTIME_ASSISTANT_POSITION_STORAGE_KEY = 'realtimeAssistant.window.position';

interface WindowPosition {
  x: number;
  y: number;
}

interface WindowSize {
  width: number;
  height: number;
}

export function defaultRealtimeAssistantPosition(
  mainPosition: WindowPosition,
  mainSize: WindowSize,
  assistantSize: WindowSize,
  scaleFactor: number,
): WindowPosition {
  const bottomInset = Math.round(88 * scaleFactor);
  return {
    x: Math.round(mainPosition.x + Math.max(0, (mainSize.width - assistantSize.width) / 2)),
    y: Math.round(
      mainPosition.y + Math.max(0, mainSize.height - assistantSize.height - bottomInset),
    ),
  };
}

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
