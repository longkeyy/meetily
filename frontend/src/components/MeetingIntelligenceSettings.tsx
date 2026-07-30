'use client';

import { useEffect, useMemo, useState } from 'react';
import { LoaderCircle, RotateCcw, Save, Sparkles } from 'lucide-react';
import { toast } from 'sonner';
import { useConfig } from '@/contexts/ConfigContext';
import { configService, ModelConfig } from '@/services/configService';
import { meetingIntelligenceService } from '@/services/meetingIntelligenceService';
import { MeetingIntelligenceSettings as SettingsValue } from '@/types/meeting-intelligence';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Switch } from './ui/switch';
import { Textarea } from './ui/textarea';

const PROVIDERS: Array<{ value: ModelConfig['provider']; label: string }> = [
  { value: 'builtin-ai', label: 'Built-in AI' },
  { value: 'claude', label: 'Claude' },
  { value: 'custom-openai', label: 'Custom OpenAI' },
  { value: 'groq', label: 'Groq' },
  { value: 'ollama', label: 'Ollama' },
  { value: 'openai', label: 'OpenAI' },
  { value: 'openrouter', label: 'OpenRouter' },
];

const LOCAL_PROVIDERS = new Set<ModelConfig['provider']>(['builtin-ai', 'ollama']);

export function MeetingIntelligenceSettings() {
  const { modelConfig: summaryModel, modelOptions } = useConfig();
  const [settings, setSettings] = useState<SettingsValue | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [isTestingConnection, setIsTestingConnection] = useState(false);

  useEffect(() => {
    void meetingIntelligenceService.getSettings()
      .then(setSettings)
      .catch((error) => {
        console.error('Failed to load meeting intelligence settings:', error);
        toast.error('Failed to load meeting intelligence settings');
      });
  }, []);

  const provider = settings?.provider ?? summaryModel.provider;
  const modelSuggestions = useMemo(() => {
    const options = new Set(modelOptions[provider] ?? []);
    if (summaryModel.provider === provider && summaryModel.model) options.add(summaryModel.model);
    return [...options];
  }, [modelOptions, provider, summaryModel.model, summaryModel.provider]);

  if (!settings) {
    return <div className="py-12 text-center text-sm text-gray-500">Loading meeting intelligence settings...</div>;
  }

  const update = <K extends keyof SettingsValue>(key: K, value: SettingsValue[K]) => {
    setSettings((current) => current ? { ...current, [key]: value } : current);
  };

  const setModelMode = (modelMode: SettingsValue['modelMode']) => {
    setSettings((current) => {
      if (!current) return current;
      if (modelMode === 'followSummary') return { ...current, modelMode };
      return {
        ...current,
        modelMode,
        provider: current.provider ?? summaryModel.provider,
        model: current.model ?? summaryModel.model,
        ollamaEndpoint: current.ollamaEndpoint ?? summaryModel.ollamaEndpoint ?? null,
      };
    });
  };

  const handleProviderChange = (nextProvider: ModelConfig['provider']) => {
    const options = modelOptions[nextProvider] ?? [];
    setSettings((current) => current ? {
      ...current,
      provider: nextProvider,
      model: summaryModel.provider === nextProvider ? summaryModel.model : options[0] ?? '',
    } : current);
  };

  const save = async () => {
    setIsSaving(true);
    try {
      const custom = settings.modelMode === 'custom';
      const saved = await meetingIntelligenceService.saveSettings({
        modelMode: settings.modelMode,
        provider: custom ? settings.provider : null,
        model: custom ? settings.model?.trim() || null : null,
        apiKey: custom ? settings.apiKey?.trim() || null : null,
        ollamaEndpoint: custom && provider === 'ollama'
          ? settings.ollamaEndpoint?.trim() || null
          : null,
        customOpenAIBaseUrl: custom && provider === 'custom-openai'
          ? settings.customOpenAIBaseUrl?.trim() || null
          : null,
        customOpenAIApiKey: custom && provider === 'custom-openai'
          ? settings.customOpenAIApiKey?.trim() || null
          : null,
        intelligentTranscriptEnabled: settings.intelligentTranscriptEnabled,
        intelligentTranscriptPrompt: settings.intelligentTranscriptPrompt,
        realtimeSummaryEnabled: settings.realtimeSummaryEnabled,
        realtimeSummaryIntervalSeconds: settings.realtimeSummaryIntervalSeconds,
        realtimeSummaryPrompt: settings.realtimeSummaryPrompt,
      });
      setSettings(saved);
      toast.success('Meeting Notes settings saved');
    } catch (error) {
      toast.error(String(error));
    } finally {
      setIsSaving(false);
    }
  };

  const testCustomOpenAI = async () => {
    const baseUrl = settings.customOpenAIBaseUrl?.trim() ?? '';
    const model = settings.model?.trim() ?? '';
    if (!baseUrl || !model) {
      toast.error('Enter the Custom OpenAI base URL and model first');
      return;
    }
    setIsTestingConnection(true);
    try {
      const result = await configService.testCustomOpenAIConnection(
        baseUrl,
        settings.customOpenAIApiKey?.trim() || null,
        model,
      );
      toast.success(result.message || 'Connection successful');
    } catch (error) {
      toast.error(String(error));
    } finally {
      setIsTestingConnection(false);
    }
  };

  return (
    <div className="flex flex-col gap-4 pb-8">
      <section className="border-b border-gray-200 py-6">
        <h3 className="text-lg font-semibold text-gray-900">Meeting Notes Model</h3>
        <p className="mt-1 text-sm text-gray-600">Used by both the detailed record and realtime summary.</p>
        <div className="mt-4 inline-flex rounded-md border border-gray-200 bg-gray-50 p-1">
          <Button
            type="button"
            size="sm"
            variant={settings.modelMode === 'followSummary' ? 'default' : 'ghost'}
            onClick={() => setModelMode('followSummary')}
          >
            Follow Summary
          </Button>
          <Button
            type="button"
            size="sm"
            variant={settings.modelMode === 'custom' ? 'default' : 'ghost'}
            onClick={() => setModelMode('custom')}
          >
            Independent
          </Button>
        </div>

        {settings.modelMode === 'followSummary' ? (
          <div className="mt-4 flex items-center gap-3 border-t border-gray-100 pt-4">
            <Sparkles className="size-4 shrink-0 text-emerald-600" aria-hidden="true" />
            <div className="min-w-0">
              <div className="text-sm font-medium text-gray-900">{summaryModel.model}</div>
              <div className="text-xs uppercase text-gray-500">{summaryModel.provider}</div>
            </div>
          </div>
        ) : (
          <div className="mt-5 grid gap-4 sm:grid-cols-2">
            <div>
              <Label>Provider</Label>
              <Select value={provider} onValueChange={(value) => handleProviderChange(value as ModelConfig['provider'])}>
                <SelectTrigger className="mt-1"><SelectValue /></SelectTrigger>
                <SelectContent>
                  {PROVIDERS.map((item) => (
                    <SelectItem key={item.value} value={item.value}>{item.label}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div>
              <Label htmlFor="meeting-notes-model">Model</Label>
              <Input
                id="meeting-notes-model"
                list="meeting-notes-model-suggestions"
                value={settings.model ?? ''}
                onChange={(event) => update('model', event.target.value)}
                placeholder="Model identifier"
                className="mt-1"
              />
              <datalist id="meeting-notes-model-suggestions">
                {modelSuggestions.map((model) => <option key={model} value={model} />)}
              </datalist>
            </div>

            {provider === 'ollama' && (
              <div className="sm:col-span-2">
                <Label htmlFor="meeting-notes-ollama-endpoint">Ollama endpoint</Label>
                <Input
                  id="meeting-notes-ollama-endpoint"
                  type="url"
                  value={settings.ollamaEndpoint ?? ''}
                  onChange={(event) => update('ollamaEndpoint', event.target.value)}
                  placeholder="http://localhost:11434"
                  className="mt-1"
                />
              </div>
            )}

            {provider === 'custom-openai' ? (
              <>
                <div className="sm:col-span-2">
                  <Label htmlFor="meeting-notes-custom-openai-base-url">Base URL</Label>
                  <Input
                    id="meeting-notes-custom-openai-base-url"
                    type="url"
                    value={settings.customOpenAIBaseUrl ?? ''}
                    onChange={(event) => update('customOpenAIBaseUrl', event.target.value)}
                    placeholder="http://localhost:8000/v1"
                    className="mt-1"
                  />
                </div>
                <div>
                  <Label htmlFor="meeting-notes-custom-openai-api-key">API key</Label>
                  <Input
                    id="meeting-notes-custom-openai-api-key"
                    type="password"
                    value={settings.customOpenAIApiKey ?? ''}
                    onChange={(event) => update('customOpenAIApiKey', event.target.value)}
                    placeholder="Optional for local services"
                    className="mt-1"
                  />
                </div>
                <div className="flex items-end">
                  <Button type="button" variant="outline" onClick={() => void testCustomOpenAI()} disabled={isTestingConnection}>
                    {isTestingConnection && <LoaderCircle className="mr-2 size-4 animate-spin" aria-hidden="true" />}
                    Test connection
                  </Button>
                </div>
              </>
            ) : !LOCAL_PROVIDERS.has(provider) && (
              <div className="sm:col-span-2">
                <Label htmlFor="meeting-notes-api-key">API key</Label>
                <Input
                  id="meeting-notes-api-key"
                  type="password"
                  value={settings.apiKey ?? ''}
                  onChange={(event) => update('apiKey', event.target.value)}
                  className="mt-1"
                />
              </div>
            )}
          </div>
        )}
      </section>

      <section className="border-b border-gray-200 py-6">
        <div className="flex items-center justify-between gap-6">
          <div>
            <h3 className="text-lg font-semibold text-gray-900">Intelligent Detailed Record</h3>
            <p className="mt-1 text-sm text-gray-600">Create a cleaned, chronological record while preserving speaker and mic roles.</p>
          </div>
          <Switch
            checked={settings.intelligentTranscriptEnabled}
            onCheckedChange={(enabled) => update('intelligentTranscriptEnabled', enabled)}
            aria-label="Enable intelligent detailed record"
          />
        </div>
      </section>

      <section className="border-b border-gray-200 py-6">
        <div className="flex items-center justify-between gap-6">
          <div>
            <h3 className="text-lg font-semibold text-gray-900">Realtime Summary</h3>
            <p className="mt-1 text-sm text-gray-600">Append a separate summary for each meeting interval.</p>
          </div>
          <Switch
            checked={settings.realtimeSummaryEnabled}
            onCheckedChange={(enabled) => update('realtimeSummaryEnabled', enabled)}
            aria-label="Enable realtime summary"
          />
        </div>
        <div className="mt-6 max-w-sm">
          <div className="flex items-center justify-between text-sm">
            <label htmlFor="realtime-summary-interval" className="font-medium text-gray-900">Summary interval</label>
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
            onChange={(event) => update('realtimeSummaryIntervalSeconds', Number(event.target.value))}
            className="mt-3 w-full accent-blue-600"
          />
        </div>
      </section>

      <PromptEditor
        title="Realtime Summary Prompt"
        value={settings.realtimeSummaryPrompt}
        defaultValue={settings.defaultRealtimeSummaryPrompt}
        rows={13}
        onChange={(value) => update('realtimeSummaryPrompt', value)}
      />
      <PromptEditor
        title="Detailed Record Prompt"
        value={settings.intelligentTranscriptPrompt}
        defaultValue={settings.defaultIntelligentTranscriptPrompt}
        rows={16}
        onChange={(value) => update('intelligentTranscriptPrompt', value)}
      />

      <div className="flex justify-end">
        <Button onClick={() => void save()} disabled={isSaving}>
          <Save className="mr-2 size-4" aria-hidden="true" />
          {isSaving ? 'Saving...' : 'Save Settings'}
        </Button>
      </div>
    </div>
  );
}

function PromptEditor({
  title,
  value,
  defaultValue,
  rows,
  onChange,
}: {
  title: string;
  value: string;
  defaultValue: string;
  rows: number;
  onChange: (value: string) => void;
}) {
  return (
    <section className="py-2">
      <div className="flex items-center justify-between gap-4">
        <h3 className="text-base font-semibold text-gray-900">{title}</h3>
        <Button type="button" variant="outline" size="sm" onClick={() => onChange(defaultValue)} disabled={value === defaultValue}>
          <RotateCcw className="mr-2 size-4" aria-hidden="true" />
          Restore default
        </Button>
      </div>
      <Textarea
        value={value}
        onChange={(event) => onChange(event.target.value)}
        rows={rows}
        maxLength={8000}
        className="mt-4 resize-y font-mono text-sm leading-5"
        aria-label={title}
      />
      <div className="mt-2 text-right text-xs tabular-nums text-gray-500">{value.length} / 8000</div>
    </section>
  );
}
