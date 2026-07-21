'use client';

import { Copy, LoaderCircle, Mic2, RefreshCw, Sparkles, Volume2 } from 'lucide-react';
import { toast } from 'sonner';
import { useConfig } from '@/contexts/ConfigContext';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { useConversationAssistant } from '@/hooks/useConversationAssistant';
import { AssistantState } from '@/lib/conversation-assistant';
import { Button } from './ui/button';
import { Switch } from './ui/switch';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip';

const EXTERNAL_PROVIDERS = new Set(['openai', 'claude', 'groq', 'openrouter', 'custom-openai']);

export function InterviewAssistantPanel() {
  const { transcripts } = useTranscripts();
  const { isRecording, isPaused } = useRecordingState();
  const { modelConfig } = useConfig();
  const { state, setEnabled, refreshSuggestion } = useConversationAssistant({
    isRecording,
    isPaused,
    transcripts,
  });

  if (!isRecording) return null;

  const handleEnabledChange = (enabled: boolean) => {
    if (enabled && EXTERNAL_PROVIDERS.has(modelConfig.provider)) {
      const acknowledgementKey = `conversationAssistant.privacyAcknowledged.${modelConfig.provider}`;
      if (localStorage.getItem(acknowledgementKey) !== 'true') {
        toast.info('Interview Assistant enabled', {
          description: `Live transcript context will be sent to the configured ${modelConfig.provider} provider.`,
          duration: 7000,
        });
        localStorage.setItem(acknowledgementKey, 'true');
      }
    }
    setEnabled(enabled);
  };

  const copySuggestion = async () => {
    if (!state.suggestion) return;
    await navigator.clipboard.writeText(state.suggestion);
    toast.success('Suggestion copied');
  };

  return (
    <InterviewAssistantPanelView
      state={state}
      hasTranscripts={transcripts.length > 0}
      onEnabledChange={handleEnabledChange}
      onRefresh={refreshSuggestion}
      onCopy={copySuggestion}
    />
  );
}

interface InterviewAssistantPanelViewProps {
  state: AssistantState;
  hasTranscripts: boolean;
  onEnabledChange: (enabled: boolean) => void;
  onRefresh: () => void;
  onCopy: () => void;
}

export function InterviewAssistantPanelView({
  state,
  hasTranscripts,
  onEnabledChange,
  onRefresh,
  onCopy,
}: InterviewAssistantPanelViewProps) {
  const status = state.micActive
    ? { icon: Mic2, label: 'Listening to you' }
    : state.status === 'generating'
      ? { icon: LoaderCircle, label: 'Preparing response' }
      : state.speakerActive
        ? { icon: Volume2, label: 'Listening to interviewer' }
        : { icon: Sparkles, label: state.suggestion ? 'Suggested response' : 'Waiting for interviewer' };
  const StatusIcon = status.icon;

  return (
    <div className="w-full overflow-hidden rounded-md border border-gray-200 bg-white shadow-lg">
      <div className="flex h-12 items-center gap-3 px-3">
        <Sparkles className="size-4 shrink-0 text-emerald-600" aria-hidden="true" />
        <span className="min-w-0 flex-1 truncate text-sm font-medium text-gray-900">
          Interview Assistant
        </span>
        {state.enabled && (
          <div className="flex items-center gap-1">
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="size-8"
                  onClick={onRefresh}
                  disabled={!hasTranscripts || state.micActive}
                >
                  <RefreshCw className="size-4" aria-hidden="true" />
                  <span className="sr-only">Refresh suggestion</span>
                </Button>
              </TooltipTrigger>
              <TooltipContent>Refresh suggestion</TooltipContent>
            </Tooltip>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="size-8"
                  onClick={onCopy}
                  disabled={!state.suggestion}
                >
                  <Copy className="size-4" aria-hidden="true" />
                  <span className="sr-only">Copy suggestion</span>
                </Button>
              </TooltipTrigger>
              <TooltipContent>Copy suggestion</TooltipContent>
            </Tooltip>
          </div>
        )}
        <Switch
          checked={state.enabled}
          onCheckedChange={onEnabledChange}
          aria-label="Enable Interview Assistant"
        />
      </div>

      {state.enabled && (
        <div className="min-h-[76px] border-t border-gray-100 px-3 py-2">
          <div className="mb-1 flex items-center gap-2 text-xs text-gray-500">
            <StatusIcon
              className={`size-3.5 ${state.status === 'generating' ? 'animate-spin' : ''}`}
              aria-hidden="true"
            />
            <span>{state.status === 'error' ? 'Assistant unavailable' : status.label}</span>
          </div>
          <div className="max-h-[68px] overflow-y-auto pr-1 text-sm leading-5 text-gray-800">
            {state.suggestion ?? (
              <span className="text-gray-400">
                {state.status === 'error'
                  ? 'Check the configured summary model and try again.'
                  : 'Suggestions will appear here.'}
              </span>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
