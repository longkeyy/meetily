import { describe, expect, test } from 'bun:test';
import {
  formatSummaryTime,
  refinedTranscriptText,
  realtimeSummaryMarkdown,
} from '../../src/types/meeting-intelligence';
import { completedTurnRevision } from '../../src/lib/refined-transcript';
import type { IntelligentTranscriptDocument, RealtimeSummaryDocument } from '../../src/types/meeting-intelligence';
import type { Transcript } from '../../src/types';

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

describe('refined transcript turns', () => {
  const transcript = (sequenceId: number, source: 'mic' | 'system', text = 'text'): Transcript => ({
    id: `segment-${sequenceId}`,
    text,
    timestamp: '12:00:00',
    sequence_id: sequenceId,
    source,
  });

  test('closes only turns followed by a different source', () => {
    expect(completedTurnRevision([
      transcript(1, 'system'),
      transcript(2, 'system'),
    ])).toBe(0);
    expect(completedTurnRevision([
      transcript(1, 'system'),
      transcript(2, 'system'),
      transcript(3, 'mic'),
    ])).toBe(2);
    expect(completedTurnRevision([
      transcript(1, 'system'),
      transcript(2, 'mic'),
      transcript(3, 'system'),
    ])).toBe(2);
  });

  test('ignores partial and empty transcripts at a source boundary', () => {
    expect(completedTurnRevision([
      transcript(1, 'system'),
      { ...transcript(2, 'mic'), is_partial: true },
      transcript(3, 'mic', '   '),
    ])).toBe(0);
  });

  test('copies turns with stable timestamps and source labels', () => {
    const document: IntelligentTranscriptDocument = {
      version: 2,
      coveredUntil: 8,
      sourceRevision: 2,
      updatedAt: '2026-07-30T00:00:00Z',
      turns: [
        {
          schemaVersion: 1,
          turnId: 'speaker-turn',
          source: 'speaker',
          startSeconds: 1,
          endSeconds: 4,
          sourceRevisionStart: 1,
          sourceRevisionEnd: 1,
          rawText: 'raw question',
          content: 'Refined question',
          createdAt: '2026-07-30T00:00:00Z',
          model: { provider: 'ollama', model: 'test' },
          promptHash: 'sha256:test',
        },
        {
          schemaVersion: 1,
          turnId: 'mic-turn',
          source: 'mic',
          startSeconds: 5,
          endSeconds: 8,
          sourceRevisionStart: 2,
          sourceRevisionEnd: 2,
          rawText: 'raw answer',
          content: 'Refined answer',
          createdAt: '2026-07-30T00:00:01Z',
          model: { provider: 'ollama', model: 'test' },
          promptHash: 'sha256:test',
        },
      ],
    };
    expect(refinedTranscriptText(document)).toBe(
      '[00:01] speaker: Refined question\n\n[00:05] mic: Refined answer',
    );
  });
});
