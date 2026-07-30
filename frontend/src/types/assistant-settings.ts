import { ModelConfig } from '@/services/configService';

export type AssistantProfileId = 'interview';
export type AssistantModelMode = 'followSummary' | 'custom';

export interface AssistantSettings {
  enabledByDefault: boolean;
  profile: AssistantProfileId;
  intervalSeconds: number;
  modelMode: AssistantModelMode;
  provider: ModelConfig['provider'] | null;
  model: string | null;
  customOpenAIBaseUrl: string | null;
  customOpenAIApiKey: string | null;
  systemPrompt: string;
  defaultSystemPrompt: string;
  isConfigured: boolean;
}

export type AssistantSettingsUpdate = Omit<
  AssistantSettings,
  'defaultSystemPrompt' | 'isConfigured'
>;

export const FALLBACK_ASSISTANT_SETTINGS: AssistantSettings = {
  enabledByDefault: true,
  profile: 'interview',
  intervalSeconds: 30,
  modelMode: 'followSummary',
  provider: null,
  model: null,
  customOpenAIBaseUrl: null,
  customOpenAIApiKey: null,
  systemPrompt: '',
  defaultSystemPrompt: '',
  isConfigured: false,
};
