import { describe, expect, test } from 'bun:test';
import {
  formatSummaryTime,
  realtimeSummaryMarkdown,
} from '../../src/types/meeting-intelligence';
import type { RealtimeSummaryDocument } from '../../src/types/meeting-intelligence';

describe('realtime summary presentation', () => {
  test('formats minute and hour timestamps', () => {
    expect(formatSummaryTime(0)).toBe('00:00');
    expect(formatSummaryTime(125.9)).toBe('02:05');
    expect(formatSummaryTime(3_725)).toBe('01:02:05');
  });

  test('copies JSONL segments as chronological markdown', () => {
    const document: RealtimeSummaryDocument = {
      version: 1,
      coveredUntil: 180,
      sourceRevision: 4,
      updatedAt: '2026-07-30T00:00:00Z',
      segments: [
        {
          schemaVersion: 1,
          segmentId: 'first',
          startSeconds: 0,
          endSeconds: 120,
          sourceRevisionStart: 1,
          sourceRevisionEnd: 3,
          contentFormat: 'markdown',
          content: 'First topic',
          trigger: 'interval',
          createdAt: '2026-07-30T00:00:00Z',
          model: { provider: 'ollama', model: 'test' },
          promptHash: 'sha256:test',
        },
        {
          schemaVersion: 1,
          segmentId: 'second',
          startSeconds: 120,
          endSeconds: 180,
          sourceRevisionStart: 4,
          sourceRevisionEnd: 4,
          contentFormat: 'markdown',
          content: 'Second topic',
          trigger: 'meetingEnd',
          createdAt: '2026-07-30T00:01:00Z',
          model: { provider: 'ollama', model: 'test' },
          promptHash: 'sha256:test',
        },
      ],
    };

    expect(realtimeSummaryMarkdown(document)).toBe(
      '## 00:00 - 02:00\n\nFirst topic\n\n## 02:00 - 03:00\n\nSecond topic',
    );
  });
});
