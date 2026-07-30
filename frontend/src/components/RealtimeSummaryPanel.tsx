'use client';

import { Copy, LoaderCircle, RefreshCw } from 'lucide-react';
import { toast } from 'sonner';
import { useRealtimeSummaryRecorder } from '@/hooks/useRealtimeSummaryRecorder';
import { Button } from '@/components/ui/button';
import { RealtimeSummarySegments } from '@/components/RealtimeSummarySegments';
import { realtimeSummaryMarkdown } from '@/types/meeting-intelligence';

export function RealtimeSummaryPanel() {
  const { document, status, error, refresh, enabled } = useRealtimeSummaryRecorder();
  const isGenerating = status === 'generating';

  const copy = async () => {
    if (!document) return;
    await navigator.clipboard.writeText(realtimeSummaryMarkdown(document));
    toast.success('Realtime summary copied');
  };

  return (
    <aside className="hidden w-[360px] shrink-0 flex-col border-l border-gray-200 bg-white lg:flex">
      <div className="flex h-14 items-center justify-between border-b border-gray-200 px-4">
        <div>
          <h2 className="text-sm font-semibold text-gray-900">Realtime Summary</h2>
          {document && (
            <p className="mt-0.5 text-xs text-gray-500">
              {new Date(document.updatedAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
            </p>
          )}
        </div>
        <div className="flex items-center gap-1">
          <Button variant="ghost" size="icon" onClick={copy} disabled={!document} title="Copy realtime summary">
            <Copy className="size-4" aria-hidden="true" />
          </Button>
          <Button variant="ghost" size="icon" onClick={() => void refresh(true)} disabled={isGenerating || !enabled} title="Refresh realtime summary">
            {isGenerating
              ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" />
              : <RefreshCw className="size-4" aria-hidden="true" />}
          </Button>
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
        {document ? (
          <RealtimeSummarySegments document={document} compact />
        ) : (
          <div className="flex h-full items-center justify-center text-center text-sm text-gray-500">
            {isGenerating ? 'Generating realtime summary...' : error || (enabled ? 'Waiting for the first summary...' : 'Realtime summary is disabled')}
          </div>
        )}
      </div>
    </aside>
  );
}
