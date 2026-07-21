import { invoke } from '@tauri-apps/api/core';
import {
  AssistantSettings,
  AssistantSettingsUpdate,
} from '@/types/assistant-settings';

export const assistantSettingsService = {
  get(): Promise<AssistantSettings> {
    return invoke<AssistantSettings>('api_get_assistant_settings');
  },

  save(settingsUpdate: AssistantSettingsUpdate): Promise<AssistantSettings> {
    return invoke<AssistantSettings>('api_save_assistant_settings', { settingsUpdate });
  },
};
