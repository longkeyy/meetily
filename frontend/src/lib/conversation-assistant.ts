export type AssistantProfile = string;
export type SuggestionTrigger = 'periodic' | 'turnEnd' | 'manual';
export type AssistantStatus = 'disabled' | 'waiting' | 'listening' | 'speaking' | 'generating' | 'ready' | 'error';

export interface AssistantTriggerCheckpoint {
  trigger: SuggestionTrigger;
  focusStartTime: number;
  targetEndTime: number;
}

export interface ReadyAssistantTrigger {
  trigger: AssistantTriggerCheckpoint;
  remaining: AssistantTriggerCheckpoint[];
}

export function periodicSuggestionTrigger(
  turnStartTime: number,
  targetEndTime: number,
): AssistantTriggerCheckpoint {
  return {
    trigger: 'periodic',
    focusStartTime: turnStartTime,
    targetEndTime,
  };
}

export function turnEndSuggestionTrigger(
  turnStartTime: number,
  turnEndTime: number,
): AssistantTriggerCheckpoint {
  return {
    trigger: 'turnEnd',
    focusStartTime: turnStartTime,
    targetEndTime: turnEndTime,
  };
}

export function enqueueSuggestionTrigger(
  pending: AssistantTriggerCheckpoint[],
  next: AssistantTriggerCheckpoint,
): AssistantTriggerCheckpoint[] {
  if (next.trigger === 'turnEnd') return [next];
  if (pending.some((item) => item.trigger === next.trigger && item.targetEndTime === next.targetEndTime)) {
    return pending;
  }
  return [...pending, next].sort((left, right) => left.targetEndTime - right.targetEndTime);
}

export function takeReadySuggestionTrigger(
  pending: AssistantTriggerCheckpoint[],
  latestSystemEnd: number,
  coverageToleranceSeconds: number,
  allowIncomplete = false,
): ReadyAssistantTrigger | null {
  const ready = pending.filter((item) => {
    const hasTranscript = latestSystemEnd >= item.focusStartTime;
    const hasCoverage = latestSystemEnd >= item.targetEndTime - coverageToleranceSeconds;
    return hasTranscript && (hasCoverage || allowIncomplete);
  });
  const trigger = ready.at(-1);
  if (!trigger) return null;

  return {
    trigger,
    remaining: pending.filter((item) => item.targetEndTime > trigger.targetEndTime),
  };
}

export interface AssistantState {
  enabled: boolean;
  profile: AssistantProfile;
  speakerActive: boolean;
  micActive: boolean;
  status: AssistantStatus;
  suggestion: string | null;
  error: string | null;
  activeRequestId: string | null;
}

export type AssistantAction =
  | { type: 'settingsLoaded'; enabled: boolean; profile: AssistantProfile }
  | { type: 'setEnabled'; enabled: boolean }
  | { type: 'sourceActivity'; source: 'mic' | 'system'; active: boolean }
  | { type: 'generationStarted'; requestId: string }
  | { type: 'generationSucceeded'; requestId: string; suggestion: string }
  | { type: 'generationFailed'; requestId: string; error: string }
  | { type: 'generationCancelled' }
  | { type: 'reset' };

export const initialAssistantState: AssistantState = {
  enabled: false,
  profile: 'interview',
  speakerActive: false,
  micActive: false,
  status: 'disabled',
  suggestion: null,
  error: null,
  activeRequestId: null,
};

function idleStatus(state: Pick<AssistantState, 'enabled' | 'speakerActive' | 'micActive' | 'suggestion'>): AssistantStatus {
  if (!state.enabled) return 'disabled';
  if (state.micActive) return 'speaking';
  if (state.speakerActive) return 'listening';
  return state.suggestion ? 'ready' : 'waiting';
}

export function assistantReducer(state: AssistantState, action: AssistantAction): AssistantState {
  switch (action.type) {
    case 'settingsLoaded': {
      const next = {
        ...state,
        enabled: action.enabled,
        profile: action.profile,
        activeRequestId: null,
        error: null,
      };
      return { ...next, status: idleStatus(next) };
    }
    case 'setEnabled': {
      const next = {
        ...state,
        enabled: action.enabled,
        activeRequestId: null,
        error: null,
      };
      return { ...next, status: idleStatus(next) };
    }
    case 'sourceActivity': {
      const next = action.source === 'mic'
        ? { ...state, micActive: action.active, error: null }
        : { ...state, speakerActive: action.active, error: null };
      const generationContinues = next.activeRequestId !== null
        && !(action.source === 'mic' && action.active);
      return {
        ...next,
        activeRequestId: action.source === 'mic' && action.active ? null : next.activeRequestId,
        status: generationContinues ? 'generating' : idleStatus(next),
      };
    }
    case 'generationStarted':
      return {
        ...state,
        activeRequestId: action.requestId,
        status: 'generating',
        error: null,
      };
    case 'generationSucceeded':
      if (state.activeRequestId !== action.requestId) return state;
      return {
        ...state,
        activeRequestId: null,
        suggestion: action.suggestion,
        status: 'ready',
        error: null,
      };
    case 'generationFailed':
      if (state.activeRequestId !== action.requestId) return state;
      return {
        ...state,
        activeRequestId: null,
        status: 'error',
        error: action.error,
      };
    case 'generationCancelled': {
      const next = { ...state, activeRequestId: null, error: null };
      return { ...next, status: idleStatus(next) };
    }
    case 'reset':
      return { ...initialAssistantState, enabled: state.enabled, status: state.enabled ? 'waiting' : 'disabled' };
  }
}
