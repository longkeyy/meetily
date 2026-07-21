import { describe, expect, test } from 'bun:test';
import {
  assistantReducer,
  initialAssistantState,
} from '../../src/lib/conversation-assistant';

describe('conversation assistant state', () => {
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
