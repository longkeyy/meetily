import { describe, expect, test } from 'bun:test';
import { defaultRealtimeAssistantPosition } from '../../src/types/realtime-assistant-overlay';

describe('Realtime Assistant window placement', () => {
  test('places the first floating window inside the main window above the recording controls', () => {
    expect(defaultRealtimeAssistantPosition(
      { x: 454, y: 187 },
      { width: 1100, height: 700 },
      { width: 500, height: 170 },
      1,
    )).toEqual({ x: 754, y: 629 });
  });

  test('keeps the top-left corner inside a main window smaller than the assistant', () => {
    expect(defaultRealtimeAssistantPosition(
      { x: 100, y: 80 },
      { width: 320, height: 120 },
      { width: 500, height: 170 },
      2,
    )).toEqual({ x: 100, y: 80 });
  });
});
