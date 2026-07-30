'use client';

import { useEffect, useState } from 'react';
import { emitTo, listen } from '@tauri-apps/api/event';
import { PhysicalPosition } from '@tauri-apps/api/dpi';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Pin, PinOff, X } from 'lucide-react';
import { toast } from 'sonner';
import { initialAssistantState } from '@/lib/conversation-assistant';
import {
  REALTIME_ASSISTANT_ACTION_EVENT,
  REALTIME_ASSISTANT_STATE_EVENT,
} from '@/types/realtime-assistant-overlay';
import type {
  RealtimeAssistantSnapshot,
  RealtimeAssistantWindowAction,
} from '@/types/realtime-assistant-overlay';
import { Button } from './ui/button';
import { RealtimeAssistantPanelView } from './InterviewAssistantPanel';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip';

const PINNED_STORAGE_KEY = 'realtimeAssistant.window.pinned';
const POSITION_STORAGE_KEY = 'realtimeAssistant.window.position';

const EMPTY_SNAPSHOT: RealtimeAssistantSnapshot = {
  state: initialAssistantState,
  profileName: 'Interview Assistant',
  scheduleState: {
    nextSuggestionAt: null,
    waitingForTranscript: false,
  },
  settingsReady: false,
  hasTranscripts: false,
};

async function sendAction(action: RealtimeAssistantWindowAction) {
  await emitTo('main', REALTIME_ASSISTANT_ACTION_EVENT, action);
}

export function RealtimeAssistantFloatingWindow() {
  const [snapshot, setSnapshot] = useState(EMPTY_SNAPSHOT);
  const [pinned, setPinned] = useState(true);

  useEffect(() => {
    const appWindow = getCurrentWindow();
    let disposed = false;
    let unlistenState: (() => void) | undefined;
    let unlistenMoved: (() => void) | undefined;

    const initialize = async () => {
      const storedPinned = localStorage.getItem(PINNED_STORAGE_KEY) !== 'false';
      setPinned(storedPinned);
      await appWindow.setAlwaysOnTop(storedPinned);

      const storedPosition = localStorage.getItem(POSITION_STORAGE_KEY);
      if (storedPosition) {
        try {
          const position = JSON.parse(storedPosition) as { x: number; y: number };
          if (Number.isFinite(position.x) && Number.isFinite(position.y)) {
            await appWindow.setPosition(new PhysicalPosition(position.x, position.y));
          }
        } catch {
          localStorage.removeItem(POSITION_STORAGE_KEY);
        }
      }

      const disposeState = await listen<RealtimeAssistantSnapshot>(
        REALTIME_ASSISTANT_STATE_EVENT,
        (event) => setSnapshot(event.payload),
      );
      if (disposed) {
        disposeState();
        return;
      }
      unlistenState = disposeState;

      const disposeMoved = await appWindow.onMoved(({ payload }) => {
        localStorage.setItem(POSITION_STORAGE_KEY, JSON.stringify({ x: payload.x, y: payload.y }));
      });
      if (disposed) {
        disposeMoved();
        return;
      }
      unlistenMoved = disposeMoved;
      await sendAction({ type: 'requestState' });
    };

    void initialize().catch((error) => {
      console.error('Failed to initialize Realtime Assistant window:', error);
    });

    return () => {
      disposed = true;
      unlistenState?.();
      unlistenMoved?.();
    };
  }, []);

  const handlePinChange = async () => {
    const nextPinned = !pinned;
    await getCurrentWindow().setAlwaysOnTop(nextPinned);
    localStorage.setItem(PINNED_STORAGE_KEY, String(nextPinned));
    setPinned(nextPinned);
  };

  const copySuggestion = async () => {
    if (!snapshot.state.suggestion) return;
    await navigator.clipboard.writeText(snapshot.state.suggestion);
    toast.success('Suggestion copied');
  };

  const startDragging = (event: React.MouseEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement;
    if (target.closest('button, [role="switch"]')) return;
    void getCurrentWindow().startDragging();
  };

  const windowActions = (
    <>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="size-8"
            onClick={() => void handlePinChange()}
          >
            {pinned ? <Pin className="size-4" aria-hidden="true" /> : <PinOff className="size-4" aria-hidden="true" />}
            <span className="sr-only">{pinned ? 'Disable always on top' : 'Keep window on top'}</span>
          </Button>
        </TooltipTrigger>
        <TooltipContent>{pinned ? 'Disable always on top' : 'Keep window on top'}</TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="size-8"
            onClick={() => void getCurrentWindow().hide()}
          >
            <X className="size-4" aria-hidden="true" />
            <span className="sr-only">Hide floating window</span>
          </Button>
        </TooltipTrigger>
        <TooltipContent>Hide floating window</TooltipContent>
      </Tooltip>
    </>
  );

  return (
    <main className="h-screen w-screen overflow-hidden bg-white">
      <RealtimeAssistantPanelView
        state={snapshot.state}
        profileName={snapshot.profileName}
        scheduleState={snapshot.scheduleState}
        settingsReady={snapshot.settingsReady}
        hasTranscripts={snapshot.hasTranscripts}
        onEnabledChange={(enabled) => void sendAction({ type: 'setEnabled', enabled })}
        onRefresh={() => void sendAction({ type: 'refresh' })}
        onCopy={() => void copySuggestion()}
        headerActions={windowActions}
        onHeaderMouseDown={startDragging}
        className="h-full rounded-none border-0 shadow-none"
      />
    </main>
  );
}
