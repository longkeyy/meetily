'use client';

import { useEffect, useState } from 'react';
import { Copy, LoaderCircle, Mic2, RefreshCw, Sparkles, Volume2 } from 'lucide-react';
import { toast } from 'sonner';
import { useConfig } from '@/contexts/ConfigContext';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useTranscripts } from '@/contexts/TranscriptContext';
import {
  AssistantScheduleState,
  useConversationAssistant,
} from '@/hooks/useConversationAssistant';
import { AssistantState } from '@/lib/conversation-assistant';
import { activeAssistantProfile } from '@/types/assistant-settings';
import { Button } from './ui/button';
import { Switch } from './ui/switch';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip';

const EXTERNAL_PROVIDERS = new Set(['openai', 'claude', 'groq', 'openrouter', 'custom-openai']);

function useCountdownSeconds(deadline: number | null): number | null {
  const [remaining, setRemaining] = useState<number | null>(null);
  useEffect(() => {
    if (deadline === null) {
      setRemaining(null);
      return;
    }
    const update = () => setRemaining(Math.max(0, Math.ceil((deadline - Date.now()) / 1_000)));
    update();
    const timer = window.setInterval(update, 250);
    return () => window.clearInterval(timer);
  }, [deadline]);
  return remaining;
}

function formatCountdown(seconds: number): string {
  const minutes = Math.floor(seconds / 60).toString().padStart(2, '0');
  const remainder = (seconds % 60).toString().padStart(2, '0');
  return `${minutes}:${remainder}`;
}

export function RealtimeAssistantPanel() {
  const { transcripts } = useTranscripts();
  const { isRecording, isPaused } = useRecordingState();
  const { modelConfig } = useConfig();
  const { state, settings, settingsReady, scheduleState, setEnabled, refreshSuggestion } = useConversationAssistant({
    isRecording,
    isPaused,
    transcripts,
  });

  if (!isRecording) return null;

  const handleEnabledChange = (enabled: boolean) => {
    const profile = activeAssistantProfile(settings);
    const effectiveProvider = profile.modelMode === 'custom'
      ? profile.provider
      : modelConfig.provider;
    if (enabled && effectiveProvider && EXTERNAL_PROVIDERS.has(effectiveProvider)) {
      const acknowledgementKey = `conversationAssistant.privacyAcknowledged.${effectiveProvider}`;
      if (localStorage.getItem(acknowledgementKey) !== 'true') {
        toast.info('Realtime Assistant enabled', {
          description: `Live transcript context will be sent to the configured ${effectiveProvider} provider.`,
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
    <RealtimeAssistantPanelView
      state={state}
      profileName={activeAssistantProfile(settings).name}
      scheduleState={scheduleState}
      settingsReady={settingsReady}
      hasTranscripts={transcripts.length > 0}
      onEnabledChange={handleEnabledChange}
      onRefresh={refreshSuggestion}
      onCopy={copySuggestion}
    />
  );
}

interface RealtimeAssistantPanelViewProps {
  state: AssistantState;
  profileName: string;
  scheduleState: AssistantScheduleState;
  settingsReady: boolean;
  hasTranscripts: boolean;
  onEnabledChange: (enabled: boolean) => void;
  onRefresh: () => void;
  onCopy: () => void;
}

export function RealtimeAssistantPanelView({
  state,
  profileName,
  scheduleState,
  settingsReady,
  hasTranscripts,
  onEnabledChange,
  onRefresh,
  onCopy,
}: RealtimeAssistantPanelViewProps) {
  const countdown = useCountdownSeconds(scheduleState.nextSuggestionAt);
  const status = state.micActive
    ? { icon: Mic2, label: 'Listening to you' }
    : state.status === 'generating'
      ? { icon: LoaderCircle, label: 'Generating suggestion' }
      : scheduleState.waitingForTranscript
        ? { icon: Volume2, label: 'Waiting for transcript' }
        : state.speakerActive && countdown !== null
          ? { icon: Volume2, label: `Next suggestion in ${formatCountdown(countdown)}` }
          : state.speakerActive
            ? { icon: Volume2, label: 'Listening to speaker' }
            : { icon: Sparkles, label: 'Waiting for speaker' };
  const StatusIcon = status.icon;

  return (
    <div className="w-full overflow-hidden rounded-md border border-gray-200 bg-white shadow-lg">
      <div className="flex h-12 items-center gap-3 px-3">
        <Sparkles className="size-4 shrink-0 text-emerald-600" aria-hidden="true" />
        <span className="min-w-0 flex-1 truncate text-sm font-medium text-gray-900">
          Realtime Assistant
        </span>
        <span className="hidden max-w-40 truncate text-xs text-gray-500 sm:block">{profileName}</span>
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
          disabled={!settingsReady}
          aria-label="Enable Realtime Assistant"
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
            {state.status === 'error' && state.error ? (
              <span className="text-red-600">{state.error}</span>
            ) : state.suggestion ?? (
              <span className="text-gray-400">
                Suggestions will appear here.
              </span>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

export const InterviewAssistantPanel = RealtimeAssistantPanel;
export const InterviewAssistantPanelView = RealtimeAssistantPanelView;
