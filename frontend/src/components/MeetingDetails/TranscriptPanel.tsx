"use client";

import { Transcript, TranscriptSegmentData } from '@/types';
import { TranscriptView } from '@/components/TranscriptView';
import { VirtualizedTranscriptView } from '@/components/VirtualizedTranscriptView';
import { TranscriptButtonGroup } from './TranscriptButtonGroup';
import { useEffect, useMemo, useState } from 'react';
import { FileText, LoaderCircle, RefreshCw, Rows3 } from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { toast } from 'sonner';
import { meetingIntelligenceService } from '@/services/meetingIntelligenceService';
import { IntelligentTranscriptDocument } from '@/types/meeting-intelligence';
import { Button } from '@/components/ui/button';

interface TranscriptPanelProps {
  transcripts: Transcript[];
  customPrompt: string;
  onPromptChange: (value: string) => void;
  onCopyTranscript: () => void;
  onOpenMeetingFolder: () => Promise<void>;
  isRecording: boolean;
  disableAutoScroll?: boolean;

  // Optional pagination props (when using virtualization)
  usePagination?: boolean;
  segments?: TranscriptSegmentData[];
  hasMore?: boolean;
  isLoadingMore?: boolean;
  totalCount?: number;
  loadedCount?: number;
  onLoadMore?: () => void;

  // Retranscription props
  meetingId?: string;
  meetingFolderPath?: string | null;
  onRefetchTranscripts?: () => Promise<void>;
}

export function TranscriptPanel({
  transcripts,
  customPrompt,
  onPromptChange,
  onCopyTranscript,
  onOpenMeetingFolder,
  isRecording,
  disableAutoScroll = false,
  usePagination = false,
  segments,
  hasMore,
  isLoadingMore,
  totalCount,
  loadedCount,
  onLoadMore,
  meetingId,
  meetingFolderPath,
  onRefetchTranscripts,
}: TranscriptPanelProps) {
  const [view, setView] = useState<'raw' | 'detailed'>('raw');
  const [detailedRecord, setDetailedRecord] = useState<IntelligentTranscriptDocument | null>(null);
  const [isLoadingDetailed, setIsLoadingDetailed] = useState(false);
  const [isRegenerating, setIsRegenerating] = useState(false);

  // Convert transcripts to segments if pagination is not used but we want virtualization
  const convertedSegments = useMemo(() => {
    if (usePagination && segments) {
      return segments;
    }
    // Convert transcripts to segments for virtualization
    return transcripts.map(t => ({
      id: t.id,
      timestamp: t.audio_start_time ?? 0,
      endTime: t.audio_end_time,
      text: t.text,
      confidence: t.confidence,
      source: t.source,
    }));
  }, [transcripts, usePagination, segments]);

  useEffect(() => {
    if (!meetingId) return;
    let disposed = false;
    setIsLoadingDetailed(true);
    void meetingIntelligenceService.getForMeeting(meetingId)
      .then((document) => {
        if (!disposed) setDetailedRecord(document);
      })
      .catch((error) => console.warn('Failed to load intelligent detailed record:', error))
      .finally(() => {
        if (!disposed) setIsLoadingDetailed(false);
      });
    return () => {
      disposed = true;
    };
  }, [meetingId]);

  const copyCurrentView = async () => {
    if (view === 'raw') {
      onCopyTranscript();
      return;
    }
    if (!detailedRecord?.markdown) return;
    await navigator.clipboard.writeText(detailedRecord.markdown);
    toast.success('Detailed record copied');
  };

  const regenerateDetailedRecord = async () => {
    if (!meetingId) return;
    setIsRegenerating(true);
    try {
      const document = await meetingIntelligenceService.regenerateForMeeting(meetingId);
      setDetailedRecord(document);
      toast.success('Detailed record regenerated');
    } catch (error) {
      toast.error('Failed to regenerate detailed record', { description: String(error) });
    } finally {
      setIsRegenerating(false);
    }
  };

  return (
    <div className="hidden md:flex md:w-1/4 lg:w-1/3 min-w-0 border-r border-gray-200 bg-white flex-col relative shrink-0">
      {/* Title area */}
      <div className="p-4 border-b border-gray-200">
        <div className="mb-3 grid grid-cols-2 rounded-md border border-gray-200 bg-gray-50 p-1">
          <button
            type="button"
            onClick={() => setView('raw')}
            className={`flex h-8 items-center justify-center gap-2 rounded text-sm ${view === 'raw' ? 'bg-white font-medium text-gray-900 shadow-sm' : 'text-gray-500'}`}
          >
            <Rows3 className="size-4" aria-hidden="true" />
            Raw
          </button>
          <button
            type="button"
            onClick={() => setView('detailed')}
            className={`flex h-8 items-center justify-center gap-2 rounded text-sm ${view === 'detailed' ? 'bg-white font-medium text-gray-900 shadow-sm' : 'text-gray-500'}`}
          >
            <FileText className="size-4" aria-hidden="true" />
            Detailed
          </button>
        </div>
        <TranscriptButtonGroup
          transcriptCount={usePagination ? (totalCount ?? convertedSegments.length) : (transcripts?.length || 0)}
          onCopyTranscript={copyCurrentView}
          onOpenMeetingFolder={onOpenMeetingFolder}
          meetingId={meetingId}
          meetingFolderPath={meetingFolderPath}
          onRefetchTranscripts={onRefetchTranscripts}
        />
      </div>

      {/* Transcript content - use virtualized view for better performance */}
      <div className="flex-1 overflow-hidden pb-4">
        {view === 'raw' ? <VirtualizedTranscriptView
          segments={convertedSegments}
          isRecording={isRecording}
          isPaused={false}
          isProcessing={false}
          isStopping={false}
          enableStreaming={false}
          showConfidence={true}
          disableAutoScroll={disableAutoScroll}
          hasMore={hasMore}
          isLoadingMore={isLoadingMore}
          totalCount={totalCount}
          loadedCount={loadedCount}
          onLoadMore={onLoadMore}
        /> : (
          <div className="h-full overflow-y-auto px-5 py-4">
            {isLoadingDetailed ? (
              <div className="flex h-32 items-center justify-center text-sm text-gray-500">
                <LoaderCircle className="mr-2 size-4 animate-spin" aria-hidden="true" />
                Loading detailed record...
              </div>
            ) : detailedRecord ? (
              <>
                <div className="mb-4 flex items-center justify-between gap-3 text-xs text-gray-500">
                  <span>Updated {new Date(detailedRecord.updatedAt).toLocaleString()}</span>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    className="size-8"
                    onClick={regenerateDetailedRecord}
                    disabled={isRegenerating}
                    title="Regenerate detailed record"
                  >
                    <RefreshCw className={`size-4 ${isRegenerating ? 'animate-spin' : ''}`} aria-hidden="true" />
                    <span className="sr-only">Regenerate detailed record</span>
                  </Button>
                </div>
                <div className="prose prose-sm max-w-none text-gray-800 prose-headings:text-gray-900">
                  <ReactMarkdown remarkPlugins={[remarkGfm]}>{detailedRecord.markdown}</ReactMarkdown>
                </div>
              </>
            ) : (
              <div className="flex h-full min-h-48 flex-col items-center justify-center text-center">
                <FileText className="size-6 text-gray-400" aria-hidden="true" />
                <p className="mt-3 text-sm font-medium text-gray-700">No detailed record yet</p>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="mt-4"
                  onClick={regenerateDetailedRecord}
                  disabled={isRegenerating || !meetingId}
                >
                  {isRegenerating && <LoaderCircle className="mr-2 size-4 animate-spin" aria-hidden="true" />}
                  Generate detailed record
                </Button>
              </div>
            )}
          </div>
        )}
      </div>

      {/* Custom prompt input at bottom of transcript section */}
      {!isRecording && convertedSegments.length > 0 && (
        <div className="p-1 border-t border-gray-200">
          <textarea
            placeholder="Add context for AI summary. For example people involved, meeting overview, objective etc..."
            className="w-full px-3 py-2 border border-gray-200 rounded-md text-sm focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500 bg-white shadow-sm min-h-[80px] resize-y"
            value={customPrompt}
            onChange={(e) => onPromptChange(e.target.value)}
          />
        </div>
      )}
    </div>
  );
}
