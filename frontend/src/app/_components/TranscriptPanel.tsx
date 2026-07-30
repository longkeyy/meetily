import { VirtualizedTranscriptView } from '@/components/VirtualizedTranscriptView';
import { PermissionWarning } from '@/components/PermissionWarning';
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import { AlertCircle, Copy, FileText, GlobeIcon, LoaderCircle, Rows3 } from 'lucide-react';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { useConfig } from '@/contexts/ConfigContext';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { usePermissionCheck } from '@/hooks/usePermissionCheck';
import { ModalType } from '@/hooks/useModalState';
import { useIsLinux } from '@/hooks/usePlatform';
import { useMemo } from 'react';
import { useState } from 'react';
import type { RefinedTranscriptState } from '@/hooks/useIntelligentTranscriptRecorder';
import { refinedTranscriptText } from '@/types/meeting-intelligence';

/**
 * TranscriptPanel Component
 *
 * Displays transcript content with controls for copying and language settings.
 * Uses TranscriptContext, ConfigContext, and RecordingStateContext internally.
 */

interface TranscriptPanelProps {
  // indicates stop-processing state for transcripts; derived from backend statuses.
  isProcessingStop: boolean;
  isStopping: boolean;
  showModal: (name: ModalType, message?: string) => void;
  refinedTranscript: RefinedTranscriptState;
}

export function TranscriptPanel({
  isProcessingStop,
  isStopping,
  showModal,
  refinedTranscript,
}: TranscriptPanelProps) {
  // Contexts
  const { transcripts, transcriptContainerRef, copyTranscript } = useTranscripts();
  const { transcriptModelConfig } = useConfig();
  const { isRecording, isPaused } = useRecordingState();
  const { checkPermissions, isChecking, hasSystemAudio, hasMicrophone } = usePermissionCheck();
  const isLinux = useIsLinux();
  const [view, setView] = useState<'original' | 'refined'>('original');

  // Convert transcripts to segments for virtualized view
  const segments = useMemo(() =>
    transcripts.map(t => ({
      id: t.id,
      timestamp: t.audio_start_time ?? 0,
      endTime: t.audio_end_time,
      text: t.text,
      confidence: t.confidence,
      source: t.source,
    })),
    [transcripts]
  );

  const refinedSegments = useMemo(() =>
    refinedTranscript.document?.turns.map((turn) => ({
      id: turn.turnId,
      timestamp: turn.startSeconds,
      endTime: turn.endSeconds,
      text: turn.content,
      source: turn.source === 'mic' ? 'mic' as const : 'system' as const,
    })) ?? [],
    [refinedTranscript.document]
  );

  const copyCurrentView = async () => {
    if (view === 'original') {
      await copyTranscript();
      return;
    }
    if (!refinedTranscript.document) return;
    await navigator.clipboard.writeText(refinedTranscriptText(refinedTranscript.document));
  };

  return (
    <div ref={transcriptContainerRef} className="w-full border-r border-gray-200 bg-white flex flex-col overflow-y-auto">
      {/* Title area - Sticky header */}
      <div className="sticky top-0 z-10 bg-white p-4 border-gray-200">
        <div className="flex flex-col space-y-3">
          <div className="mx-auto grid w-full max-w-[360px] grid-cols-2 rounded-md border border-gray-200 bg-gray-50 p-1">
            <button
              type="button"
              onClick={() => setView('original')}
              className={`flex h-8 items-center justify-center gap-2 rounded text-sm ${view === 'original' ? 'bg-white font-medium text-gray-900 shadow-sm' : 'text-gray-500'}`}
            >
              <Rows3 className="size-4" aria-hidden="true" />
              Original
            </button>
            <button
              type="button"
              onClick={() => setView('refined')}
              className={`flex h-8 items-center justify-center gap-2 rounded text-sm ${view === 'refined' ? 'bg-white font-medium text-gray-900 shadow-sm' : 'text-gray-500'}`}
            >
              <FileText className="size-4" aria-hidden="true" />
              Refined
            </button>
          </div>
          <div className="flex  flex-col space-y-2">
            <div className="flex justify-center  items-center space-x-2">
              <ButtonGroup>
                {transcripts?.length > 0 && (
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => void copyCurrentView()}
                    title="Copy Transcript"
                  >
                    <Copy />
                    <span className='hidden md:inline'>
                      Copy
                    </span>
                  </Button>
                )}
                {transcriptModelConfig.provider === "localWhisper" &&
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => showModal('languageSettings')}
                    title="Language"
                  >
                    <GlobeIcon />
                    <span className='hidden md:inline'>
                      Language
                    </span>
                  </Button>
                }
              </ButtonGroup>
            </div>
          </div>
        </div>
      </div>

      {/* Permission Warning - Not needed on Linux */}
      {!isRecording && !isChecking && !isLinux && (
        <div className="flex justify-center px-4 pt-4">
          <PermissionWarning
            hasMicrophone={hasMicrophone}
            hasSystemAudio={hasSystemAudio}
            onRecheck={checkPermissions}
            isRechecking={isChecking}
          />
        </div>
      )}

      {/* Transcript content */}
      <div className="pb-20">
        <div className="flex justify-center">
          <div className="w-2/3 max-w-[750px]">
            {view === 'refined' && refinedTranscript.status === 'generating' && (
              <div className="mb-4 flex items-center justify-center gap-2 text-sm text-gray-500">
                <LoaderCircle className="size-4 animate-spin" aria-hidden="true" />
                Refining the completed turn...
              </div>
            )}
            {view === 'refined' && refinedTranscript.status === 'waiting' && refinedSegments.length === 0 && (
              <div className="mb-4 text-center text-sm text-gray-500">
                A refined turn appears after the speaker changes.
              </div>
            )}
            {view === 'refined' && refinedTranscript.status === 'error' && (
              <div className="mb-4 flex items-center justify-center gap-2 text-sm text-red-600" title={refinedTranscript.error ?? undefined}>
                <AlertCircle className="size-4" aria-hidden="true" />
                Refining failed; the original transcript is still available.
              </div>
            )}
            {view === 'refined' && refinedSegments.length === 0 ? (
              <div className="flex min-h-48 flex-col items-center justify-center px-6 text-center text-gray-500">
                <FileText className="size-6 text-gray-400" aria-hidden="true" />
                <p className="mt-3 text-sm font-medium text-gray-700">No completed turn yet</p>
                <p className="mt-1 text-xs">The Original view remains available while the first turn is refined.</p>
              </div>
            ) : (
              <VirtualizedTranscriptView
                segments={view === 'original' ? segments : refinedSegments}
                isRecording={isRecording && view === 'original'}
                isPaused={isPaused}
                isProcessing={isProcessingStop}
                isStopping={isStopping}
                enableStreaming={isRecording && view === 'original'}
                showConfidence={view === 'original'}
              />
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
