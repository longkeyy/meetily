'use client';

import { useEffect, useMemo, useState } from 'react';
import { LoaderCircle, Plus, RotateCcw, Save, Sparkles, Trash2 } from 'lucide-react';
import { toast } from 'sonner';
import { useConfig } from '@/contexts/ConfigContext';
import { assistantSettingsService } from '@/services/assistantSettingsService';
import { configService, ModelConfig } from '@/services/configService';
import {
  AssistantProfileSettings,
  AssistantSettings as AssistantSettingsValue,
  AssistantSettingsUpdate,
  activeAssistantProfile,
} from '@/types/assistant-settings';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Switch } from './ui/switch';
import { Textarea } from './ui/textarea';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip';

const PROVIDERS: Array<{ value: ModelConfig['provider']; label: string }> = [
  { value: 'builtin-ai', label: 'Built-in AI' },
  { value: 'claude', label: 'Claude' },
  { value: 'custom-openai', label: 'Custom OpenAI' },
  { value: 'groq', label: 'Groq' },
  { value: 'ollama', label: 'Ollama' },
  { value: 'openai', label: 'OpenAI' },
  { value: 'openrouter', label: 'OpenRouter' },
];

function toUpdate(settings: AssistantSettingsValue): AssistantSettingsUpdate {
  return {
    enabledByDefault: settings.enabledByDefault,
    activeProfileId: settings.activeProfileId,
    profiles: settings.profiles.map(({ defaultSystemPrompt: _, ...profile }) => profile),
  };
}

function uniqueProfileId(profiles: AssistantProfileSettings[]): string {
  const existing = new Set(profiles.map((profile) => profile.id));
  let suffix = Date.now().toString(36);
  let id = `custom-${suffix}`;
  while (existing.has(id)) {
    suffix = `${suffix}-1`;
    id = `custom-${suffix}`;
  }
  return id;
}

export function AssistantSettings() {
  const { modelConfig: summaryModel, modelOptions } = useConfig();
  const [settings, setSettings] = useState<AssistantSettingsValue | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [isTestingConnection, setIsTestingConnection] = useState(false);

  useEffect(() => {
    let disposed = false;
    void assistantSettingsService.get()
      .then((loaded) => {
        if (!disposed) setSettings(loaded);
      })
      .catch((error) => {
        console.error('Failed to load realtime assistant settings:', error);
        toast.error('Failed to load realtime assistant settings');
      });
    return () => {
      disposed = true;
    };
  }, []);

  const activeProfile = settings ? activeAssistantProfile(settings) : null;
  const provider = activeProfile?.provider ?? summaryModel.provider;
  const modelSuggestions = useMemo(() => {
    const options = new Set(modelOptions[provider] ?? []);
    if (summaryModel.provider === provider && summaryModel.model) {
      options.add(summaryModel.model);
    }
    return [...options];
  }, [modelOptions, provider, summaryModel.model, summaryModel.provider]);

  if (!settings || !activeProfile) {
    return <div className="py-12 text-center text-sm text-gray-500">Loading realtime assistant settings...</div>;
  }

  const updateSettings = <K extends keyof AssistantSettingsValue>(
    key: K,
    value: AssistantSettingsValue[K],
  ) => setSettings((current) => current ? { ...current, [key]: value } : current);

  const updateProfile = <K extends keyof AssistantProfileSettings>(
    key: K,
    value: AssistantProfileSettings[K],
  ) => setSettings((current) => current ? {
    ...current,
    profiles: current.profiles.map((profile) => profile.id === current.activeProfileId
      ? { ...profile, [key]: value }
      : profile),
  } : current);

  const addProfile = () => {
    setSettings((current) => {
      if (!current) return current;
      const source = activeAssistantProfile(current);
      const id = uniqueProfileId(current.profiles);
      const profile: AssistantProfileSettings = {
        ...source,
        id,
        name: 'New Assistant',
        builtIn: false,
        customOpenAIApiKey: null,
        defaultSystemPrompt: source.defaultSystemPrompt || source.systemPrompt,
      };
      return { ...current, activeProfileId: id, profiles: [...current.profiles, profile] };
    });
  };

  const deleteProfile = () => {
    if (activeProfile.builtIn) return;
    setSettings((current) => current ? {
      ...current,
      activeProfileId: 'interview',
      profiles: current.profiles.filter((profile) => profile.id !== current.activeProfileId),
    } : current);
  };

  const setModelMode = (modelMode: AssistantProfileSettings['modelMode']) => {
    updateProfile('modelMode', modelMode);
    if (modelMode === 'custom' && !activeProfile.provider) {
      setSettings((current) => current ? {
        ...current,
        profiles: current.profiles.map((profile) => profile.id === current.activeProfileId ? {
          ...profile,
          modelMode,
          provider: summaryModel.provider,
          model: profile.model ?? summaryModel.model,
        } : profile),
      } : current);
    }
  };

  const handleProviderChange = (nextProvider: ModelConfig['provider']) => {
    const options = modelOptions[nextProvider] ?? [];
    setSettings((current) => current ? {
      ...current,
      profiles: current.profiles.map((profile) => profile.id === current.activeProfileId ? {
        ...profile,
        provider: nextProvider,
        model: summaryModel.provider === nextProvider ? summaryModel.model : options[0] ?? '',
      } : profile),
    } : current);
  };

  const handleSave = async () => {
    setIsSaving(true);
    try {
      const saved = await assistantSettingsService.save(toUpdate(settings));
      setSettings(saved);
      localStorage.removeItem('conversationAssistant.interview.enabled');
      toast.success('Realtime Assistant settings saved');
    } catch (error) {
      console.error('Failed to save realtime assistant settings:', error);
      toast.error(String(error));
    } finally {
      setIsSaving(false);
    }
  };

  const handleTestConnection = async () => {
    const baseUrl = activeProfile.customOpenAIBaseUrl?.trim() ?? '';
    const model = activeProfile.model?.trim() ?? '';
    if (!baseUrl || !model) {
      toast.error('Enter the Custom OpenAI base URL and model first');
      return;
    }
    setIsTestingConnection(true);
    try {
      const result = await configService.testCustomOpenAIConnection(
        baseUrl,
        activeProfile.customOpenAIApiKey?.trim() || null,
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
    <div className="flex flex-col gap-4">
      <section className="rounded-lg border border-gray-200 bg-white p-6 shadow-sm">
        <div className="flex items-center justify-between gap-6">
          <div>
            <h3 className="text-lg font-semibold text-gray-900">Realtime Assistant</h3>
            <p className="mt-1 text-sm text-gray-600">Suggest what the microphone participant could say next.</p>
          </div>
          <Switch
            checked={settings.enabledByDefault}
            onCheckedChange={(enabled) => updateSettings('enabledByDefault', enabled)}
            aria-label="Enable Realtime Assistant by default"
          />
        </div>

        <div className="mt-5 grid gap-4 sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] sm:items-end">
          <div>
            <Label htmlFor="assistant-profile">Active profile</Label>
            <Select
              value={settings.activeProfileId}
              onValueChange={(value) => updateSettings('activeProfileId', value)}
            >
              <SelectTrigger id="assistant-profile" className="mt-1">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {settings.profiles.map((profile) => (
                  <SelectItem key={profile.id} value={profile.id}>{profile.name}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div>
            <Label htmlFor="assistant-profile-name">Profile name</Label>
            <Input
              id="assistant-profile-name"
              value={activeProfile.name}
              maxLength={80}
              onChange={(event) => updateProfile('name', event.target.value)}
              className="mt-1"
            />
          </div>
          <div className="flex gap-1">
            <Tooltip>
              <TooltipTrigger asChild>
                <Button type="button" variant="outline" size="icon" onClick={addProfile} aria-label="Add assistant profile">
                  <Plus className="size-4" aria-hidden="true" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>Add profile</TooltipContent>
            </Tooltip>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  onClick={deleteProfile}
                  disabled={activeProfile.builtIn}
                  aria-label="Delete assistant profile"
                >
                  <Trash2 className="size-4" aria-hidden="true" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>{activeProfile.builtIn ? 'Built-in profiles cannot be deleted' : 'Delete profile'}</TooltipContent>
            </Tooltip>
          </div>
        </div>
      </section>

      <section className="rounded-lg border border-gray-200 bg-white p-6 shadow-sm">
        <h3 className="text-lg font-semibold text-gray-900">Suggestion Timing</h3>
        <div className="mt-5 grid gap-4 sm:grid-cols-[1fr_96px] sm:items-end">
          <div>
            <div className="flex items-center justify-between gap-4">
              <Label htmlFor="assistant-interval">Continuous speaker interval</Label>
              <span className="text-sm tabular-nums text-gray-500">{activeProfile.intervalSeconds}s</span>
            </div>
            <input
              id="assistant-interval"
              type="range"
              min={10}
              max={120}
              step={5}
              value={activeProfile.intervalSeconds}
              onChange={(event) => updateProfile('intervalSeconds', Number(event.target.value))}
              className="mt-3 h-2 w-full cursor-pointer accent-blue-600"
            />
          </div>
          <div>
            <Label htmlFor="assistant-interval-number">Seconds</Label>
            <Input
              id="assistant-interval-number"
              type="number"
              min={10}
              max={120}
              step={5}
              value={activeProfile.intervalSeconds}
              onChange={(event) => {
                const value = Number(event.target.value);
                if (Number.isFinite(value)) updateProfile('intervalSeconds', value);
              }}
              className="mt-1 tabular-nums"
            />
          </div>
        </div>
        <div className="mt-4 flex items-center justify-between border-t border-gray-100 pt-4 text-sm">
          <span className="text-gray-600">Speaker turn completed</span>
          <span className="font-medium text-gray-900">Immediately</span>
        </div>
      </section>

      <section className="rounded-lg border border-gray-200 bg-white p-6 shadow-sm">
        <h3 className="text-lg font-semibold text-gray-900">Assistant Model</h3>
        <div className="mt-4 inline-flex rounded-md border border-gray-200 bg-gray-50 p-1">
          <Button type="button" size="sm" variant={activeProfile.modelMode === 'followSummary' ? 'default' : 'ghost'} onClick={() => setModelMode('followSummary')}>
            Follow Summary
          </Button>
          <Button type="button" size="sm" variant={activeProfile.modelMode === 'custom' ? 'default' : 'ghost'} onClick={() => setModelMode('custom')}>
            Independent
          </Button>
        </div>

        {activeProfile.modelMode === 'followSummary' ? (
          <div className="mt-4 flex items-center gap-3 rounded-md border border-gray-200 px-4 py-3">
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
                  {PROVIDERS.map((item) => <SelectItem key={item.value} value={item.value}>{item.label}</SelectItem>)}
                </SelectContent>
              </Select>
            </div>
            <div>
              <Label htmlFor="assistant-model">Model</Label>
              <Input
                id="assistant-model"
                list="assistant-model-suggestions"
                value={activeProfile.model ?? ''}
                onChange={(event) => updateProfile('model', event.target.value)}
                placeholder="Model identifier"
                className="mt-1"
              />
              <datalist id="assistant-model-suggestions">
                {modelSuggestions.map((model) => <option key={model} value={model} />)}
              </datalist>
            </div>
            {provider === 'custom-openai' && (
              <>
                <div className="sm:col-span-2">
                  <Label htmlFor="assistant-custom-openai-base-url">Base URL</Label>
                  <Input
                    id="assistant-custom-openai-base-url"
                    type="url"
                    value={activeProfile.customOpenAIBaseUrl ?? ''}
                    onChange={(event) => updateProfile('customOpenAIBaseUrl', event.target.value)}
                    placeholder="http://localhost:8000/v1"
                    className="mt-1"
                  />
                </div>
                <div>
                  <Label htmlFor="assistant-custom-openai-api-key">API key</Label>
                  <Input
                    id="assistant-custom-openai-api-key"
                    type="password"
                    value={activeProfile.customOpenAIApiKey ?? ''}
                    onChange={(event) => updateProfile('customOpenAIApiKey', event.target.value)}
                    placeholder="Optional"
                    autoComplete="off"
                    className="mt-1"
                  />
                </div>
                <div className="flex items-end">
                  <Button type="button" variant="outline" onClick={handleTestConnection} disabled={isTestingConnection}>
                    {isTestingConnection && <LoaderCircle className="mr-2 size-4 animate-spin" aria-hidden="true" />}
                    Test connection
                  </Button>
                </div>
              </>
            )}
          </div>
        )}
      </section>

      <section className="rounded-lg border border-gray-200 bg-white p-6 shadow-sm">
        <div className="flex items-center justify-between gap-4">
          <h3 className="text-lg font-semibold text-gray-900">System Prompt</h3>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => updateProfile('systemPrompt', activeProfile.defaultSystemPrompt)}
            disabled={activeProfile.systemPrompt === activeProfile.defaultSystemPrompt}
          >
            <RotateCcw className="mr-2 size-4" aria-hidden="true" />
            Restore default
          </Button>
        </div>
        <Textarea
          value={activeProfile.systemPrompt}
          onChange={(event) => updateProfile('systemPrompt', event.target.value)}
          rows={12}
          maxLength={8000}
          className="mt-4 resize-y font-mono text-sm leading-5"
          aria-label="Assistant system prompt"
        />
        <div className="mt-2 text-right text-xs tabular-nums text-gray-500">{activeProfile.systemPrompt.length} / 8000</div>
      </section>

      <div className="flex justify-end pb-8">
        <Button onClick={handleSave} disabled={isSaving}>
          <Save className="mr-2 size-4" aria-hidden="true" />
          {isSaving ? 'Saving...' : 'Save Realtime Assistant'}
        </Button>
      </div>
    </div>
  );
}
