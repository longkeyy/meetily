import { describe, expect, test } from 'bun:test';
import {
  assistantReducer,
  enqueueSuggestionTrigger,
  initialAssistantState,
  periodicSuggestionTrigger,
  takeReadySuggestionTrigger,
  turnEndSuggestionTrigger,
} from '../../src/lib/conversation-assistant';

describe('conversation assistant state', () => {
  test('loads the persisted profile and enabled preference', () => {
    const state = assistantReducer(initialAssistantState, {
      type: 'settingsLoaded',
      enabled: true,
      profile: 'interview',
    });
    expect(state.enabled).toBe(true);
    expect(state.profile).toBe('interview');
    expect(state.status).toBe('waiting');
  });

  test('tracks speaker and microphone activity independently', () => {
    let state = assistantReducer(initialAssistantState, { type: 'setEnabled', enabled: true });
    state = assistantReducer(state, { type: 'sourceActivity', source: 'system', active: true });
    expect(state.status).toBe('listening');

    state = assistantReducer(state, { type: 'sourceActivity', source: 'mic', active: true });
    expect(state.status).toBe('speaking');

    state = assistantReducer(state, { type: 'sourceActivity', source: 'mic', active: false });
    expect(state.status).toBe('listening');
  });

  test('ignores stale generations and keeps only the latest suggestion', () => {
    let state = assistantReducer(initialAssistantState, { type: 'setEnabled', enabled: true });
    state = assistantReducer(state, { type: 'generationStarted', requestId: 'first' });
    state = assistantReducer(state, { type: 'generationStarted', requestId: 'second' });
    state = assistantReducer(state, {
      type: 'generationSucceeded',
      requestId: 'first',
      suggestion: 'stale',
    });
    expect(state.suggestion).toBeNull();

    state = assistantReducer(state, {
      type: 'generationSucceeded',
      requestId: 'second',
      suggestion: 'latest',
    });
    expect(state.suggestion).toBe('latest');
  });

  test('microphone activity cancels generation without clearing the last suggestion', () => {
    let state = {
      ...initialAssistantState,
      enabled: true,
      status: 'ready' as const,
      suggestion: 'Describe the trade-off first.',
    };
    state = assistantReducer(state, { type: 'generationStarted', requestId: 'pending' });
    state = assistantReducer(state, { type: 'sourceActivity', source: 'mic', active: true });

    expect(state.activeRequestId).toBeNull();
    expect(state.suggestion).toBe('Describe the trade-off first.');
    expect(state.status).toBe('speaking');
  });

  test('speaker activity updates do not hide an active generation', () => {
    let state = assistantReducer(initialAssistantState, { type: 'setEnabled', enabled: true });
    state = assistantReducer(state, { type: 'generationStarted', requestId: 'pending' });
    state = assistantReducer(state, { type: 'sourceActivity', source: 'system', active: true });

    expect(state.activeRequestId).toBe('pending');
    expect(state.status).toBe('generating');
  });

  test('reset clears meeting data but preserves the enabled preference', () => {
    const state = assistantReducer(
      {
        ...initialAssistantState,
        enabled: true,
        status: 'ready',
        suggestion: 'Previous meeting suggestion',
      },
      { type: 'reset' },
    );

    expect(state.enabled).toBe(true);
    expect(state.suggestion).toBeNull();
    expect(state.status).toBe('waiting');
  });
});

describe('conversation assistant trigger schedule', () => {
  test('refreshes cumulatively at 30 and 60 seconds', () => {
    let pending = enqueueSuggestionTrigger([], periodicSuggestionTrigger(0, 30));
    const first = takeReadySuggestionTrigger(pending, 30, 1.5);
    expect(first?.trigger).toEqual({
      trigger: 'periodic',
      focusStartTime: 0,
      targetEndTime: 30,
    });

    pending = enqueueSuggestionTrigger(first?.remaining ?? [], periodicSuggestionTrigger(0, 60));
    const second = takeReadySuggestionTrigger(pending, 60, 1.5);
    expect(second?.trigger).toEqual({
      trigger: 'periodic',
      focusStartTime: 0,
      targetEndTime: 60,
    });
  });

  test('accepts a 15 second interval checkpoint', () => {
    const pending = enqueueSuggestionTrigger([], periodicSuggestionTrigger(5, 20));
    expect(takeReadySuggestionTrigger(pending, 18.5, 1.5)?.trigger).toEqual({
      trigger: 'periodic',
      focusStartTime: 5,
      targetEndTime: 20,
    });
  });

  test('turn end supersedes an unconsumed checkpoint and refreshes at 70 seconds', () => {
    let pending = enqueueSuggestionTrigger([], periodicSuggestionTrigger(0, 60));
    pending = enqueueSuggestionTrigger(pending, turnEndSuggestionTrigger(0, 70));

    expect(pending).toHaveLength(1);
    expect(takeReadySuggestionTrigger(pending, 70, 1.5)?.trigger).toEqual({
      trigger: 'turnEnd',
      focusStartTime: 0,
      targetEndTime: 70,
    });
  });

  test('keeps a delayed checkpoint until transcript coverage arrives', () => {
    const pending = enqueueSuggestionTrigger([], periodicSuggestionTrigger(10, 40));
    expect(takeReadySuggestionTrigger(pending, 35, 1.5)).toBeNull();
    expect(takeReadySuggestionTrigger(pending, 39, 1.5)?.trigger.targetEndTime).toBe(40);
  });
});
