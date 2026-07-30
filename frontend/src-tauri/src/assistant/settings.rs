use super::profiles::{
    default_system_prompt, INTERVIEW_PROFILE_ID, INTERVIEW_PROFILE_NAME, INTERVIEW_SYSTEM_PROMPT,
};
use anyhow::{anyhow, Result};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "assistant_settings.json";
const STORE_KEY: &str = "settings";
const SETTINGS_SCHEMA_VERSION: u32 = 2;
pub const DEFAULT_INTERVAL_SECONDS: u32 = 30;
pub const MIN_INTERVAL_SECONDS: u32 = 10;
pub const MAX_INTERVAL_SECONDS: u32 = 120;
pub const MAX_SYSTEM_PROMPT_CHARS: usize = 8_000;
const MAX_PROFILE_NAME_CHARS: usize = 80;
const MAX_PROFILES: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssistantModelMode {
    FollowSummary,
    Custom,
}

impl Default for AssistantModelMode {
    fn default() -> Self {
        Self::FollowSummary
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AssistantProfileSettings {
    pub id: String,
    pub name: String,
    pub built_in: bool,
    pub interval_seconds: u32,
    pub model_mode: AssistantModelMode,
    pub provider: Option<String>,
    pub model: Option<String>,
    #[serde(rename = "customOpenAIBaseUrl", alias = "customOpenAiBaseUrl")]
    pub custom_openai_base_url: Option<String>,
    #[serde(rename = "customOpenAIApiKey", alias = "customOpenAiApiKey")]
    pub custom_openai_api_key: Option<String>,
    pub system_prompt: String,
}

impl Default for AssistantProfileSettings {
    fn default() -> Self {
        Self::interview()
    }
}

impl AssistantProfileSettings {
    fn interview() -> Self {
        Self {
            id: INTERVIEW_PROFILE_ID.to_string(),
            name: INTERVIEW_PROFILE_NAME.to_string(),
            built_in: true,
            interval_seconds: DEFAULT_INTERVAL_SECONDS,
            model_mode: AssistantModelMode::FollowSummary,
            provider: None,
            model: None,
            custom_openai_base_url: None,
            custom_openai_api_key: None,
            system_prompt: INTERVIEW_SYSTEM_PROMPT.to_string(),
        }
    }

    fn normalize(&mut self) {
        self.id = self.id.trim().to_string();
        self.name = self.name.trim().to_string();
        self.interval_seconds = self
            .interval_seconds
            .clamp(MIN_INTERVAL_SECONDS, MAX_INTERVAL_SECONDS);
        self.provider = clean_optional(self.provider.take());
        self.model = clean_optional(self.model.take());
        self.custom_openai_base_url = clean_optional(self.custom_openai_base_url.take())
            .map(|value| value.trim_end_matches('/').to_string());
        self.custom_openai_api_key = clean_optional(self.custom_openai_api_key.take());
        self.system_prompt = self.system_prompt.trim().to_string();
        if self.id == INTERVIEW_PROFILE_ID {
            self.built_in = true;
            if self.name.is_empty() {
                self.name = INTERVIEW_PROFILE_NAME.to_string();
            }
            if self.system_prompt.is_empty() {
                self.system_prompt = INTERVIEW_SYSTEM_PROMPT.to_string();
            }
        } else {
            self.built_in = false;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AssistantSettings {
    pub schema_version: u32,
    pub enabled_by_default: bool,
    pub active_profile_id: String,
    pub profiles: Vec<AssistantProfileSettings>,
}

impl Default for AssistantSettings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            enabled_by_default: true,
            active_profile_id: INTERVIEW_PROFILE_ID.to_string(),
            profiles: vec![AssistantProfileSettings::interview()],
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantProfileView {
    pub id: String,
    pub name: String,
    pub built_in: bool,
    pub interval_seconds: u32,
    pub model_mode: AssistantModelMode,
    pub provider: Option<String>,
    pub model: Option<String>,
    #[serde(rename = "customOpenAIBaseUrl")]
    pub custom_openai_base_url: Option<String>,
    #[serde(rename = "customOpenAIApiKey")]
    pub custom_openai_api_key: Option<String>,
    pub system_prompt: String,
    pub default_system_prompt: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSettingsView {
    pub enabled_by_default: bool,
    pub active_profile_id: String,
    pub profiles: Vec<AssistantProfileView>,
    pub is_configured: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantProfileUpdate {
    pub id: String,
    pub name: String,
    pub built_in: bool,
    pub interval_seconds: u32,
    pub model_mode: AssistantModelMode,
    pub provider: Option<String>,
    pub model: Option<String>,
    #[serde(rename = "customOpenAIBaseUrl", alias = "customOpenAiBaseUrl")]
    pub custom_openai_base_url: Option<String>,
    #[serde(rename = "customOpenAIApiKey", alias = "customOpenAiApiKey")]
    pub custom_openai_api_key: Option<String>,
    pub system_prompt: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSettingsUpdate {
    pub enabled_by_default: bool,
    pub active_profile_id: String,
    pub profiles: Vec<AssistantProfileUpdate>,
}

impl AssistantSettings {
    pub fn profile(&self, id: &str) -> Option<&AssistantProfileSettings> {
        self.profiles.iter().find(|profile| profile.id == id)
    }

    pub fn live_checkpoint_ms(&self) -> u32 {
        self.profile(&self.active_profile_id)
            .map(|profile| profile.interval_seconds)
            .unwrap_or(DEFAULT_INTERVAL_SECONDS)
            .clamp(MIN_INTERVAL_SECONDS, MAX_INTERVAL_SECONDS)
            * 1_000
    }

    fn normalize(&mut self) {
        self.schema_version = SETTINGS_SCHEMA_VERSION;
        for profile in &mut self.profiles {
            profile.normalize();
        }
        if !self
            .profiles
            .iter()
            .any(|profile| profile.id == INTERVIEW_PROFILE_ID)
        {
            self.profiles
                .insert(0, AssistantProfileSettings::interview());
        }
        if self.profile(&self.active_profile_id).is_none() {
            self.active_profile_id = INTERVIEW_PROFILE_ID.to_string();
        }
    }

    fn apply_update(&mut self, update: AssistantSettingsUpdate) -> Result<()> {
        validate_update(&update)?;
        self.enabled_by_default = update.enabled_by_default;
        self.active_profile_id = update.active_profile_id;
        self.profiles = update
            .profiles
            .into_iter()
            .map(|profile| AssistantProfileSettings {
                id: profile.id,
                name: profile.name,
                built_in: profile.built_in,
                interval_seconds: profile.interval_seconds,
                model_mode: profile.model_mode,
                provider: profile.provider,
                model: profile.model,
                custom_openai_base_url: profile.custom_openai_base_url,
                custom_openai_api_key: profile.custom_openai_api_key,
                system_prompt: profile.system_prompt,
            })
            .collect();
        self.normalize();
        Ok(())
    }

    fn to_view(&self, is_configured: bool) -> AssistantSettingsView {
        AssistantSettingsView {
            enabled_by_default: self.enabled_by_default,
            active_profile_id: self.active_profile_id.clone(),
            profiles: self
                .profiles
                .iter()
                .map(|profile| AssistantProfileView {
                    id: profile.id.clone(),
                    name: profile.name.clone(),
                    built_in: profile.built_in,
                    interval_seconds: profile.interval_seconds,
                    model_mode: profile.model_mode,
                    provider: profile.provider.clone(),
                    model: profile.model.clone(),
                    custom_openai_base_url: profile.custom_openai_base_url.clone(),
                    custom_openai_api_key: profile.custom_openai_api_key.clone(),
                    system_prompt: profile.system_prompt.clone(),
                    default_system_prompt: default_system_prompt(&profile.id).to_string(),
                })
                .collect(),
            is_configured,
        }
    }
}

pub async fn load_assistant_settings<R: Runtime>(app: &AppHandle<R>) -> AssistantSettings {
    load_settings_state(app).0
}

pub async fn get_assistant_settings<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<AssistantSettingsView, String> {
    let (settings, is_configured) = load_settings_state(app);
    Ok(settings.to_view(is_configured))
}

pub async fn save_assistant_settings<R: Runtime>(
    app: &AppHandle<R>,
    update: AssistantSettingsUpdate,
) -> Result<AssistantSettingsView, String> {
    let (mut settings, _) = load_settings_state(app);
    settings
        .apply_update(update)
        .map_err(|error| error.to_string())?;

    let store = app
        .store(STORE_FILE)
        .map_err(|error| format!("Failed to access assistant settings: {error}"))?;
    let value = serde_json::to_value(&settings)
        .map_err(|error| format!("Failed to serialize assistant settings: {error}"))?;
    store.set(STORE_KEY, value);
    store
        .save()
        .map_err(|error| format!("Failed to persist assistant settings: {error}"))?;
    let view = settings.to_view(true);
    app.emit("assistant-settings-updated", view.clone())
        .map_err(|error| format!("Failed to broadcast assistant settings: {error}"))?;
    info!(
        "Saved realtime assistant settings: profile={}, profiles={}",
        settings.active_profile_id,
        settings.profiles.len()
    );
    Ok(view)
}

fn load_settings_state<R: Runtime>(app: &AppHandle<R>) -> (AssistantSettings, bool) {
    let Ok(store) = app.store(STORE_FILE) else {
        warn!("Failed to access assistant settings store; using defaults");
        return (AssistantSettings::default(), false);
    };
    let Some(value) = store.get(STORE_KEY) else {
        return (AssistantSettings::default(), false);
    };

    if value.get("schemaVersion").and_then(|value| value.as_u64())
        == Some(SETTINGS_SCHEMA_VERSION as u64)
    {
        return match serde_json::from_value::<AssistantSettings>(value.clone()) {
            Ok(mut settings) => {
                settings.normalize();
                (settings, true)
            }
            Err(error) => {
                warn!("Failed to parse realtime assistant settings: {error}; using defaults");
                (AssistantSettings::default(), false)
            }
        };
    }

    match serde_json::from_value::<LegacyAssistantSettings>(value.clone()) {
        Ok(legacy) => (legacy.migrate(), true),
        Err(error) => {
            warn!("Failed to migrate assistant settings: {error}; using defaults");
            (AssistantSettings::default(), false)
        }
    }
}

fn validate_update(update: &AssistantSettingsUpdate) -> Result<()> {
    if update.profiles.is_empty() || update.profiles.len() > MAX_PROFILES {
        return Err(anyhow!(
            "Realtime Assistant must have between 1 and {MAX_PROFILES} profiles"
        ));
    }
    let mut ids = HashSet::new();
    let mut has_interview = false;
    for profile in &update.profiles {
        let id = profile.id.trim();
        if id == INTERVIEW_PROFILE_ID {
            has_interview = true;
        }
        if id.is_empty()
            || id.len() > 80
            || !id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        {
            return Err(anyhow!(
                "Assistant profile IDs may only contain letters, numbers, '-' and '_'"
            ));
        }
        if !ids.insert(id) {
            return Err(anyhow!("Assistant profile IDs must be unique"));
        }
        let name = profile.name.trim();
        if name.is_empty() || name.chars().count() > MAX_PROFILE_NAME_CHARS {
            return Err(anyhow!(
                "Assistant profile names must be 1-{MAX_PROFILE_NAME_CHARS} characters"
            ));
        }
        if !(MIN_INTERVAL_SECONDS..=MAX_INTERVAL_SECONDS).contains(&profile.interval_seconds) {
            return Err(anyhow!(
                "Suggestion interval must be between {MIN_INTERVAL_SECONDS} and {MAX_INTERVAL_SECONDS} seconds"
            ));
        }
        if profile.system_prompt.trim().is_empty() {
            return Err(anyhow!("System prompt cannot be empty"));
        }
        if profile.system_prompt.chars().count() > MAX_SYSTEM_PROMPT_CHARS {
            return Err(anyhow!(
                "System prompt cannot exceed {MAX_SYSTEM_PROMPT_CHARS} characters"
            ));
        }
        validate_model(profile)?;
    }
    if !has_interview {
        return Err(anyhow!(
            "The built-in Interview Assistant profile cannot be deleted"
        ));
    }
    if !ids.contains(update.active_profile_id.trim()) {
        return Err(anyhow!("Select an existing assistant profile"));
    }
    Ok(())
}

fn validate_model(profile: &AssistantProfileUpdate) -> Result<()> {
    if profile.model_mode != AssistantModelMode::Custom {
        return Ok(());
    }
    let provider = profile.provider.as_deref().unwrap_or_default().trim();
    let model = profile.model.as_deref().unwrap_or_default().trim();
    crate::summary::llm_client::LLMProvider::from_str(provider).map_err(|error| anyhow!(error))?;
    if model.is_empty() {
        return Err(anyhow!("Select a model for the assistant"));
    }
    if provider.eq_ignore_ascii_case("custom-openai") {
        let base_url = profile
            .custom_openai_base_url
            .as_deref()
            .unwrap_or_default()
            .trim();
        if base_url.is_empty() {
            return Err(anyhow!("Enter a Custom OpenAI base URL for the assistant"));
        }
        if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
            return Err(anyhow!(
                "Custom OpenAI base URL must start with http:// or https://"
            ));
        }
    }
    Ok(())
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct LegacyProfileSettings {
    system_prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct LegacyAssistantSettings {
    enabled_by_default: bool,
    active_profile: String,
    interval_seconds: u32,
    model_mode: AssistantModelMode,
    provider: Option<String>,
    model: Option<String>,
    #[serde(rename = "customOpenAIBaseUrl", alias = "customOpenAiBaseUrl")]
    custom_openai_base_url: Option<String>,
    #[serde(rename = "customOpenAIApiKey", alias = "customOpenAiApiKey")]
    custom_openai_api_key: Option<String>,
    profiles: HashMap<String, LegacyProfileSettings>,
}

impl Default for LegacyAssistantSettings {
    fn default() -> Self {
        Self {
            enabled_by_default: true,
            active_profile: INTERVIEW_PROFILE_ID.to_string(),
            interval_seconds: DEFAULT_INTERVAL_SECONDS,
            model_mode: AssistantModelMode::FollowSummary,
            provider: None,
            model: None,
            custom_openai_base_url: None,
            custom_openai_api_key: None,
            profiles: HashMap::new(),
        }
    }
}

impl LegacyAssistantSettings {
    fn migrate(self) -> AssistantSettings {
        let active_profile_id = if self.active_profile == INTERVIEW_PROFILE_ID {
            self.active_profile
        } else {
            INTERVIEW_PROFILE_ID.to_string()
        };
        let prompt = self
            .profiles
            .get(INTERVIEW_PROFILE_ID)
            .and_then(|profile| profile.system_prompt.clone())
            .filter(|prompt| !prompt.trim().is_empty())
            .unwrap_or_else(|| INTERVIEW_SYSTEM_PROMPT.to_string());
        let mut interview = AssistantProfileSettings {
            interval_seconds: self.interval_seconds,
            model_mode: self.model_mode,
            provider: self.provider,
            model: self.model,
            custom_openai_base_url: self.custom_openai_base_url,
            custom_openai_api_key: self.custom_openai_api_key,
            system_prompt: prompt,
            ..AssistantProfileSettings::interview()
        };
        interview.normalize();
        AssistantSettings {
            schema_version: SETTINGS_SCHEMA_VERSION,
            enabled_by_default: self.enabled_by_default,
            active_profile_id,
            profiles: vec![interview],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str, name: &str) -> AssistantProfileUpdate {
        AssistantProfileUpdate {
            id: id.to_string(),
            name: name.to_string(),
            built_in: id == INTERVIEW_PROFILE_ID,
            interval_seconds: 15,
            model_mode: AssistantModelMode::Custom,
            provider: Some("ollama".to_string()),
            model: Some("qwen3:4b".to_string()),
            custom_openai_base_url: None,
            custom_openai_api_key: None,
            system_prompt: "Give a concise response.".to_string(),
        }
    }

    #[test]
    fn defaults_include_the_builtin_interview_profile() {
        let settings = AssistantSettings::default();
        assert!(settings.enabled_by_default);
        assert_eq!(settings.active_profile_id, INTERVIEW_PROFILE_ID);
        assert_eq!(
            settings
                .profile(INTERVIEW_PROFILE_ID)
                .unwrap()
                .interval_seconds,
            30
        );
    }

    #[test]
    fn update_supports_custom_profiles_and_active_selection() {
        let mut settings = AssistantSettings::default();
        settings
            .apply_update(AssistantSettingsUpdate {
                enabled_by_default: true,
                active_profile_id: "sales".to_string(),
                profiles: vec![
                    profile("interview", INTERVIEW_PROFILE_NAME),
                    profile("sales", "Sales Assistant"),
                ],
            })
            .unwrap();
        assert_eq!(settings.active_profile_id, "sales");
        assert_eq!(settings.profile("sales").unwrap().interval_seconds, 15);
    }

    #[test]
    fn validation_protects_the_builtin_profile_and_rejects_bad_intervals() {
        let missing_builtin = AssistantSettingsUpdate {
            enabled_by_default: true,
            active_profile_id: "sales".to_string(),
            profiles: vec![profile("sales", "Sales Assistant")],
        };
        assert!(validate_update(&missing_builtin).is_err());

        let mut interview = profile("interview", INTERVIEW_PROFILE_NAME);
        interview.interval_seconds = 5;
        assert!(validate_update(&AssistantSettingsUpdate {
            enabled_by_default: true,
            active_profile_id: "interview".to_string(),
            profiles: vec![interview],
        })
        .is_err());
    }

    #[test]
    fn legacy_single_profile_settings_migrate_without_losing_credentials() {
        let legacy: LegacyAssistantSettings = serde_json::from_value(serde_json::json!({
            "enabledByDefault": false,
            "activeProfile": "interview",
            "intervalSeconds": 15,
            "modelMode": "custom",
            "provider": "custom-openai",
            "model": "local-model",
            "customOpenAIBaseUrl": "http://localhost:1234/v1",
            "customOpenAIApiKey": "secret",
            "profiles": {"interview": {"systemPrompt": "Custom interview prompt"}}
        }))
        .unwrap();
        let settings = legacy.migrate();
        let interview = settings.profile(INTERVIEW_PROFILE_ID).unwrap();
        assert!(!settings.enabled_by_default);
        assert_eq!(interview.interval_seconds, 15);
        assert_eq!(interview.custom_openai_api_key.as_deref(), Some("secret"));
        assert_eq!(interview.system_prompt, "Custom interview prompt");
    }

    #[test]
    fn follow_summary_profiles_retain_independent_provider_credentials() {
        let mut interview = profile("interview", INTERVIEW_PROFILE_NAME);
        interview.model_mode = AssistantModelMode::FollowSummary;
        interview.custom_openai_api_key = Some("keep-me".to_string());
        let mut settings = AssistantSettings::default();
        settings
            .apply_update(AssistantSettingsUpdate {
                enabled_by_default: true,
                active_profile_id: "interview".to_string(),
                profiles: vec![interview],
            })
            .unwrap();
        assert_eq!(
            settings
                .profile("interview")
                .unwrap()
                .custom_openai_api_key
                .as_deref(),
            Some("keep-me")
        );
    }

    #[test]
    fn openai_fields_use_the_frontend_contract_and_accept_legacy_casing() {
        let value = serde_json::to_value(AssistantSettings::default()).unwrap();
        let profile = &value["profiles"][0];
        assert!(profile.get("customOpenAIBaseUrl").is_some());
        assert!(profile.get("customOpenAIApiKey").is_some());

        let legacy: LegacyAssistantSettings = serde_json::from_value(serde_json::json!({
            "customOpenAiBaseUrl": "http://localhost:11434/v1",
            "customOpenAiApiKey": "legacy-secret"
        }))
        .unwrap();
        let migrated = legacy.migrate();
        let interview = migrated.profile(INTERVIEW_PROFILE_ID).unwrap();
        assert_eq!(
            interview.custom_openai_api_key.as_deref(),
            Some("legacy-secret")
        );
    }
}
