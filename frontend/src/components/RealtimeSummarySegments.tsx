'use client';

import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { formatSummaryTime, RealtimeSummaryDocument } from '@/types/meeting-intelligence';

export function RealtimeSummarySegments({
  document,
  compact = false,
}: {
  document: RealtimeSummaryDocument;
  compact?: boolean;
}) {
  return (
    <div className="divide-y divide-gray-200">
      {document.segments.map((segment) => (
        <section key={segment.segmentId} className={compact ? 'py-4 first:pt-0' : 'py-6 first:pt-0'}>
          <div className="mb-3 flex items-center gap-3">
            <span className="text-xs font-semibold tabular-nums text-gray-700">
              {formatSummaryTime(segment.startSeconds)} - {formatSummaryTime(segment.endSeconds)}
            </span>
            <span className="h-px flex-1 bg-gray-200" aria-hidden="true" />
          </div>
          <article className={compact
            ? 'prose prose-sm max-w-none prose-headings:mb-2 prose-headings:mt-4 prose-headings:text-gray-900 prose-p:my-2 prose-li:my-1'
            : 'prose max-w-none prose-headings:text-gray-900'}
          >
            <ReactMarkdown remarkPlugins={[remarkGfm]}>{segment.content}</ReactMarkdown>
          </article>
        </section>
      ))}
    </div>
  );
}
