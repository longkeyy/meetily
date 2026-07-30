import { ModelConfig } from '@/services/configService';

export type AssistantProfileId = string;
export type AssistantModelMode = 'followSummary' | 'custom';

export interface AssistantProfileSettings {
  id: AssistantProfileId;
  name: string;
  builtIn: boolean;
  intervalSeconds: number;
  modelMode: AssistantModelMode;
  provider: ModelConfig['provider'] | null;
  model: string | null;
  customOpenAIBaseUrl: string | null;
  customOpenAIApiKey: string | null;
  systemPrompt: string;
  defaultSystemPrompt: string;
}

export interface AssistantSettings {
  enabledByDefault: boolean;
  activeProfileId: AssistantProfileId;
  profiles: AssistantProfileSettings[];
  isConfigured: boolean;
}

export type AssistantProfileSettingsUpdate = Omit<AssistantProfileSettings, 'defaultSystemPrompt'>;

export interface AssistantSettingsUpdate {
  enabledByDefault: boolean;
  activeProfileId: AssistantProfileId;
  profiles: AssistantProfileSettingsUpdate[];
}

const FALLBACK_INTERVIEW_PROFILE: AssistantProfileSettings = {
  id: 'interview',
  name: 'Interview Assistant',
  builtIn: true,
  intervalSeconds: 30,
  modelMode: 'followSummary',
  provider: null,
  model: null,
  customOpenAIBaseUrl: null,
  customOpenAIApiKey: null,
  systemPrompt: '',
  defaultSystemPrompt: '',
};

export const FALLBACK_ASSISTANT_SETTINGS: AssistantSettings = {
  enabledByDefault: true,
  activeProfileId: 'interview',
  profiles: [FALLBACK_INTERVIEW_PROFILE],
  isConfigured: false,
};

export function activeAssistantProfile(settings: AssistantSettings): AssistantProfileSettings {
  return settings.profiles.find((profile) => profile.id === settings.activeProfileId)
    ?? settings.profiles[0]
    ?? FALLBACK_INTERVIEW_PROFILE;
}
