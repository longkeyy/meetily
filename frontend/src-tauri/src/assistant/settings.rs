use super::{profiles::profile_definition, AssistantProfile};
use anyhow::{anyhow, Result};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "assistant_settings.json";
const STORE_KEY: &str = "settings";
pub const DEFAULT_INTERVAL_SECONDS: u32 = 30;
pub const MIN_INTERVAL_SECONDS: u32 = 10;
pub const MAX_INTERVAL_SECONDS: u32 = 120;
pub const MAX_SYSTEM_PROMPT_CHARS: usize = 8_000;

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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AssistantProfileSettings {
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AssistantSettings {
    pub enabled_by_default: bool,
    pub active_profile: AssistantProfile,
    pub interval_seconds: u32,
    pub model_mode: AssistantModelMode,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub profiles: HashMap<AssistantProfile, AssistantProfileSettings>,
}

impl Default for AssistantSettings {
    fn default() -> Self {
        Self {
            enabled_by_default: true,
            active_profile: AssistantProfile::Interview,
            interval_seconds: DEFAULT_INTERVAL_SECONDS,
            model_mode: AssistantModelMode::FollowSummary,
            provider: None,
            model: None,
            profiles: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSettingsView {
    pub enabled_by_default: bool,
    pub profile: AssistantProfile,
    pub interval_seconds: u32,
    pub model_mode: AssistantModelMode,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub system_prompt: String,
    pub default_system_prompt: String,
    pub is_configured: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSettingsUpdate {
    pub enabled_by_default: bool,
    pub profile: AssistantProfile,
    pub interval_seconds: u32,
    pub model_mode: AssistantModelMode,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub system_prompt: String,
}

impl AssistantSettings {
    pub fn resolved_system_prompt(&self, profile: AssistantProfile) -> String {
        self.profiles
            .get(&profile)
            .and_then(|settings| settings.system_prompt.as_deref())
            .filter(|prompt| !prompt.trim().is_empty())
            .unwrap_or_else(|| profile_definition(profile).system_prompt)
            .to_string()
    }

    pub fn live_checkpoint_ms(&self) -> u32 {
        self.interval_seconds
            .clamp(MIN_INTERVAL_SECONDS, MAX_INTERVAL_SECONDS)
            * 1_000
    }

    fn apply_update(&mut self, update: AssistantSettingsUpdate) -> Result<()> {
        validate_update(&update)?;
        let default_prompt = profile_definition(update.profile).system_prompt.trim();
        let prompt = update.system_prompt.trim();
        let prompt_override = (prompt != default_prompt).then(|| prompt.to_string());

        self.enabled_by_default = update.enabled_by_default;
        self.active_profile = update.profile;
        self.interval_seconds = update.interval_seconds;
        self.model_mode = update.model_mode;
        self.provider = update
            .provider
            .map(|provider| provider.trim().to_string())
            .filter(|provider| !provider.is_empty());
        self.model = update
            .model
            .map(|model| model.trim().to_string())
            .filter(|model| !model.is_empty());
        self.profiles
            .entry(update.profile)
            .or_default()
            .system_prompt = prompt_override;
        Ok(())
    }

    fn to_view(&self, is_configured: bool) -> AssistantSettingsView {
        let profile = self.active_profile;
        AssistantSettingsView {
            enabled_by_default: self.enabled_by_default,
            profile,
            interval_seconds: self.interval_seconds,
            model_mode: self.model_mode,
            provider: self.provider.clone(),
            model: self.model.clone(),
            system_prompt: self.resolved_system_prompt(profile),
            default_system_prompt: profile_definition(profile).system_prompt.to_string(),
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
    info!(
        "Saved assistant settings: profile={:?}, interval={}s, model_mode={:?}",
        settings.active_profile, settings.interval_seconds, settings.model_mode
    );
    Ok(settings.to_view(true))
}

fn load_settings_state<R: Runtime>(app: &AppHandle<R>) -> (AssistantSettings, bool) {
    let Ok(store) = app.store(STORE_FILE) else {
        warn!("Failed to access assistant settings store; using defaults");
        return (AssistantSettings::default(), false);
    };
    let Some(value) = store.get(STORE_KEY) else {
        return (AssistantSettings::default(), false);
    };

    match serde_json::from_value::<AssistantSettings>(value.clone()) {
        Ok(mut settings) => {
            settings.interval_seconds = settings
                .interval_seconds
                .clamp(MIN_INTERVAL_SECONDS, MAX_INTERVAL_SECONDS);
            (settings, true)
        }
        Err(error) => {
            warn!("Failed to parse assistant settings: {error}; using defaults");
            (AssistantSettings::default(), false)
        }
    }
}

fn validate_update(update: &AssistantSettingsUpdate) -> Result<()> {
    if !(MIN_INTERVAL_SECONDS..=MAX_INTERVAL_SECONDS).contains(&update.interval_seconds) {
        return Err(anyhow!(
            "Suggestion interval must be between {MIN_INTERVAL_SECONDS} and {MAX_INTERVAL_SECONDS} seconds"
        ));
    }
    if update.system_prompt.trim().is_empty() {
        return Err(anyhow!("System prompt cannot be empty"));
    }
    if update.system_prompt.chars().count() > MAX_SYSTEM_PROMPT_CHARS {
        return Err(anyhow!(
            "System prompt cannot exceed {MAX_SYSTEM_PROMPT_CHARS} characters"
        ));
    }
    if update.model_mode == AssistantModelMode::Custom {
        let provider = update.provider.as_deref().unwrap_or_default().trim();
        let model = update.model.as_deref().unwrap_or_default().trim();
        crate::summary::llm_client::LLMProvider::from_str(provider)
            .map_err(|error| anyhow!(error))?;
        if model.is_empty() {
            return Err(anyhow!("Select a model for the assistant"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update() -> AssistantSettingsUpdate {
        AssistantSettingsUpdate {
            enabled_by_default: true,
            profile: AssistantProfile::Interview,
            interval_seconds: 45,
            model_mode: AssistantModelMode::Custom,
            provider: Some("ollama".to_string()),
            model: Some("qwen3:4b".to_string()),
            system_prompt: "Give a concise interview response.".to_string(),
        }
    }

    #[test]
    fn defaults_follow_summary_and_use_the_interview_prompt() {
        let settings = AssistantSettings::default();
        assert!(settings.enabled_by_default);
        assert_eq!(settings.interval_seconds, DEFAULT_INTERVAL_SECONDS);
        assert_eq!(settings.model_mode, AssistantModelMode::FollowSummary);
        assert_eq!(
            settings.resolved_system_prompt(AssistantProfile::Interview),
            profile_definition(AssistantProfile::Interview).system_prompt
        );
    }

    #[test]
    fn update_persists_a_profile_specific_prompt_override() {
        let mut settings = AssistantSettings::default();
        settings.apply_update(update()).unwrap();
        assert_eq!(settings.live_checkpoint_ms(), 45_000);
        assert_eq!(
            settings.resolved_system_prompt(AssistantProfile::Interview),
            "Give a concise interview response."
        );
    }

    #[test]
    fn default_prompt_is_stored_as_no_override() {
        let mut input = update();
        input.system_prompt = profile_definition(AssistantProfile::Interview)
            .system_prompt
            .to_string();
        let mut settings = AssistantSettings::default();
        settings.apply_update(input).unwrap();
        assert!(settings
            .profiles
            .get(&AssistantProfile::Interview)
            .unwrap()
            .system_prompt
            .is_none());
    }

    #[test]
    fn profile_settings_serialize_with_stable_json_keys() {
        let mut settings = AssistantSettings::default();
        settings.apply_update(update()).unwrap();

        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"interview\""));

        let restored: AssistantSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(
            restored.resolved_system_prompt(AssistantProfile::Interview),
            "Give a concise interview response."
        );
    }

    #[test]
    fn validation_rejects_invalid_intervals_and_custom_models() {
        let mut input = update();
        input.interval_seconds = 5;
        assert!(validate_update(&input).is_err());

        input.interval_seconds = 30;
        input.model = None;
        assert!(validate_update(&input).is_err());
    }
}
