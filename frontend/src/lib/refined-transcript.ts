import type { Transcript } from '@/types';

function normalizedSource(transcript: Transcript): 'speaker' | 'mic' {
  return transcript.source === 'mic' ? 'mic' : 'speaker';
}

export function completedTurnRevision(transcripts: Transcript[]): number {
  const finalTranscripts = transcripts.filter(
    (transcript) => transcript.is_partial !== true && transcript.text.trim(),
  );
  let activeSource: 'speaker' | 'mic' | null = null;
  let activeRevision = 0;
  let completedRevision = 0;
  finalTranscripts.forEach((transcript, index) => {
    const source = normalizedSource(transcript);
    const revision = transcript.sequence_id ?? index + 1;
    if (activeSource !== null && source !== activeSource) {
      completedRevision = activeRevision;
      activeRevision = revision;
    } else {
      activeRevision = Math.max(activeRevision, revision);
    }
    activeSource = source;
  });
  return completedRevision;
}
