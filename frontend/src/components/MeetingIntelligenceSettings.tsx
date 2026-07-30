'use client';

import { useEffect, useState } from 'react';
import { RotateCcw, Save } from 'lucide-react';
import { toast } from 'sonner';
import { meetingIntelligenceService } from '@/services/meetingIntelligenceService';
import { MeetingIntelligenceSettings as SettingsValue } from '@/types/meeting-intelligence';
import { Button } from './ui/button';
import { Switch } from './ui/switch';
import { Textarea } from './ui/textarea';

export function MeetingIntelligenceSettings() {
  const [settings, setSettings] = useState<SettingsValue | null>(null);
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    void meetingIntelligenceService.getSettings()
      .then(setSettings)
      .catch((error) => {
        console.error('Failed to load meeting intelligence settings:', error);
        toast.error('Failed to load meeting intelligence settings');
      });
  }, []);

  if (!settings) {
    return <div className="py-12 text-center text-sm text-gray-500">Loading meeting intelligence settings...</div>;
  }

  const save = async () => {
    setIsSaving(true);
    try {
      const saved = await meetingIntelligenceService.saveSettings({
        intelligentTranscriptEnabled: settings.intelligentTranscriptEnabled,
        intelligentTranscriptPrompt: settings.intelligentTranscriptPrompt,
        realtimeSummaryEnabled: settings.realtimeSummaryEnabled,
        realtimeSummaryIntervalSeconds: settings.realtimeSummaryIntervalSeconds,
        realtimeSummaryPrompt: settings.realtimeSummaryPrompt,
      });
      setSettings(saved);
      toast.success('Meeting intelligence settings saved');
    } catch (error) {
      toast.error(String(error));
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="flex flex-col gap-4 pb-8">
      <section className="border-b border-gray-200 py-6">
        <div className="flex items-center justify-between gap-6">
          <div>
            <h3 className="text-lg font-semibold text-gray-900">Intelligent Detailed Record</h3>
            <p className="mt-1 text-sm text-gray-600">Create a cleaned, chronological record while preserving speaker and mic roles.</p>
          </div>
          <Switch
            checked={settings.intelligentTranscriptEnabled}
            onCheckedChange={(enabled) => setSettings((current) => current ? {
              ...current,
              intelligentTranscriptEnabled: enabled,
            } : current)}
            aria-label="Enable intelligent detailed record"
          />
        </div>
      </section>

      <section className="border-t border-gray-200 pt-6">
        <div className="flex items-center justify-between gap-6">
          <div>
            <h3 className="text-lg font-semibold text-gray-900">Realtime Summary</h3>
            <p className="mt-1 text-sm text-gray-600">Maintain a cumulative summary while the meeting is in progress.</p>
          </div>
          <Switch
            checked={settings.realtimeSummaryEnabled}
            onCheckedChange={(enabled) => setSettings((current) => current ? {
              ...current,
              realtimeSummaryEnabled: enabled,
            } : current)}
            aria-label="Enable realtime summary"
          />
        </div>

        <div className="mt-6 max-w-sm">
          <div className="flex items-center justify-between text-sm">
            <label htmlFor="realtime-summary-interval" className="font-medium text-gray-900">Refresh interval</label>
            <span className="tabular-nums text-gray-600">
              {settings.realtimeSummaryIntervalSeconds >= 60
                ? `${settings.realtimeSummaryIntervalSeconds / 60} min`
                : `${settings.realtimeSummaryIntervalSeconds} sec`}
            </span>
          </div>
          <input
            id="realtime-summary-interval"
            type="range"
            min={60}
            max={1800}
            step={30}
            value={settings.realtimeSummaryIntervalSeconds}
            onChange={(event) => setSettings((current) => current ? {
              ...current,
              realtimeSummaryIntervalSeconds: Number(event.target.value),
            } : current)}
            className="mt-3 w-full accent-blue-600"
          />
        </div>
      </section>

      <section className="py-2">
        <div className="flex items-center justify-between gap-4">
          <h3 className="text-base font-semibold text-gray-900">Realtime Summary Prompt</h3>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => setSettings((current) => current ? {
              ...current,
              realtimeSummaryPrompt: current.defaultRealtimeSummaryPrompt,
            } : current)}
            disabled={settings.realtimeSummaryPrompt === settings.defaultRealtimeSummaryPrompt}
          >
            <RotateCcw className="mr-2 size-4" aria-hidden="true" />
            Restore default
          </Button>
        </div>
        <Textarea
          value={settings.realtimeSummaryPrompt}
          onChange={(event) => setSettings((current) => current ? {
            ...current,
            realtimeSummaryPrompt: event.target.value,
          } : current)}
          rows={13}
          maxLength={8000}
          className="mt-4 resize-y font-mono text-sm leading-5"
          aria-label="Realtime summary prompt"
        />
        <div className="mt-2 text-right text-xs tabular-nums text-gray-500">
          {settings.realtimeSummaryPrompt.length} / 8000
        </div>
      </section>

      <section className="py-2">
        <div className="flex items-center justify-between gap-4">
          <h3 className="text-base font-semibold text-gray-900">Detailed Record Prompt</h3>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => setSettings((current) => current ? {
              ...current,
              intelligentTranscriptPrompt: current.defaultIntelligentTranscriptPrompt,
            } : current)}
            disabled={settings.intelligentTranscriptPrompt === settings.defaultIntelligentTranscriptPrompt}
          >
            <RotateCcw className="mr-2 size-4" aria-hidden="true" />
            Restore default
          </Button>
        </div>
        <Textarea
          value={settings.intelligentTranscriptPrompt}
          onChange={(event) => setSettings((current) => current ? {
            ...current,
            intelligentTranscriptPrompt: event.target.value,
          } : current)}
          rows={16}
          maxLength={8000}
          className="mt-4 resize-y font-mono text-sm leading-5"
          aria-label="Intelligent detailed record prompt"
        />
        <div className="mt-2 text-right text-xs tabular-nums text-gray-500">
          {settings.intelligentTranscriptPrompt.length} / 8000
        </div>
      </section>

      <div className="flex justify-end">
        <Button onClick={save} disabled={isSaving}>
          <Save className="mr-2 size-4" aria-hidden="true" />
          {isSaving ? 'Saving...' : 'Save Settings'}
        </Button>
      </div>
    </div>
  );
}
