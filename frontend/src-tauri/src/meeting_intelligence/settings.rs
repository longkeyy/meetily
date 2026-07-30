use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "meeting_intelligence_settings.json";
const STORE_KEY: &str = "settings";
const MAX_PROMPT_CHARS: usize = 8_000;
pub const DEFAULT_REALTIME_SUMMARY_INTERVAL_SECONDS: u32 = 120;
pub const MIN_REALTIME_SUMMARY_INTERVAL_SECONDS: u32 = 60;
pub const MAX_REALTIME_SUMMARY_INTERVAL_SECONDS: u32 = 1_800;

pub const DEFAULT_INTELLIGENT_TRANSCRIPT_PROMPT: &str = r#"你是会议转写整理助手。你永远不回应会议参与者，只整理输入中一个已经结束的单方发言轮次，并直接输出整理后的文本。

要求：
1. 去除“呃”“嗯”“你知道的”等填充词，以及无意义重复和口吃。
2. 识别说话人的自我更正，只保留最终表达的意图。
3. 将口述的列表、步骤和要点整理为清晰、易读的文本，但不要总结或省略有效信息。
4. 保留原有语气、事实、数字、专有名词、问题、回答、决定和未解决事项，不得杜撰。
5. 不添加讽刺、情绪化评价、能力判断或会议中没有出现的主观旁白。
6. 发言来源由应用固定管理；不要输出 speaker/mic 标签、标题或处理说明，只输出本轮整理后的内容。"#;

const LEGACY_INTELLIGENT_TRANSCRIPT_PROMPT: &str = r#"你是会议智能记录助手。你永远不回应会议参与者，只整理输入的原始转录，并直接输出完整的会议详细记录。

要求：
1. 按会议实际发生顺序，用连贯、专业、易读的叙述记录讨论流程。
2. 发言者只能使用 speaker 和 mic 两个名称，必须保留谁提问、谁回答、谁补充或确认了什么。
3. 去除“呃”“嗯”“你知道的”等填充词，以及无意义重复和口吃。
4. 识别说话人的自我更正，只保留最终表达的意图。
5. 将口述的列表、步骤和要点整理为清晰的 Markdown，但不要把详细流程压缩成只有结论的概要。
6. 保留事实、数字、专有名词、问题、回答、决定和未解决事项，不得杜撰。
7. 不添加讽刺、情绪化评价、能力判断或类似“这波”“致命伤”的主观旁白；只有参与者明确表达的评价才能记录，并注明是谁表达的。
8. 不输出“会议详细”等标题，不解释处理过程，直接输出整理后的详细记录。"#;

pub const DEFAULT_REALTIME_SUMMARY_PROMPT: &str = r#"你是会议实时摘要助手。你永远不回应会议参与者，只总结当前时间段内提供的原始转录。

使用 Markdown 且只包含以下部分：
## 讨论主题
## 结论与决定
## 行动项
## 未解决问题

要求：
1. 合并重复信息，保留事实、数字、专有名词和 speaker/mic 的明确观点。
2. 新内容纠正旧内容时采用最新的明确表述。
3. 没有结论、行动项或未解决问题时写“暂无”，不得杜撰。
4. 不加入能力评价、情绪化判断或会议中没有出现的推断。
5. 不引用或重复其他时间段的摘要，不解释处理过程，直接输出当前时间段的摘要。"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MeetingIntelligenceModelMode {
    FollowSummary,
    Custom,
}

impl Default for MeetingIntelligenceModelMode {
    fn default() -> Self {
        Self::FollowSummary
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MeetingIntelligenceSettings {
    pub model_mode: MeetingIntelligenceModelMode,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub ollama_endpoint: Option<String>,
    pub custom_openai_base_url: Option<String>,
    pub custom_openai_api_key: Option<String>,
    pub intelligent_transcript_enabled: bool,
    pub intelligent_transcript_prompt: String,
    pub realtime_summary_enabled: bool,
    pub realtime_summary_interval_seconds: u32,
    pub realtime_summary_prompt: String,
}

impl Default for MeetingIntelligenceSettings {
    fn default() -> Self {
        Self {
            model_mode: MeetingIntelligenceModelMode::FollowSummary,
            provider: None,
            model: None,
            api_key: None,
            ollama_endpoint: None,
            custom_openai_base_url: None,
            custom_openai_api_key: None,
            intelligent_transcript_enabled: true,
            intelligent_transcript_prompt: DEFAULT_INTELLIGENT_TRANSCRIPT_PROMPT.to_string(),
            realtime_summary_enabled: true,
            realtime_summary_interval_seconds: DEFAULT_REALTIME_SUMMARY_INTERVAL_SECONDS,
            realtime_summary_prompt: DEFAULT_REALTIME_SUMMARY_PROMPT.to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingIntelligenceSettingsUpdate {
    pub model_mode: MeetingIntelligenceModelMode,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub ollama_endpoint: Option<String>,
    pub custom_openai_base_url: Option<String>,
    pub custom_openai_api_key: Option<String>,
    pub intelligent_transcript_enabled: bool,
    pub intelligent_transcript_prompt: String,
    pub realtime_summary_enabled: bool,
    pub realtime_summary_interval_seconds: u32,
    pub realtime_summary_prompt: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingIntelligenceSettingsView {
    pub model_mode: MeetingIntelligenceModelMode,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub ollama_endpoint: Option<String>,
    pub custom_openai_base_url: Option<String>,
    pub custom_openai_api_key: Option<String>,
    pub intelligent_transcript_enabled: bool,
    pub intelligent_transcript_prompt: String,
    pub default_intelligent_transcript_prompt: String,
    pub realtime_summary_enabled: bool,
    pub realtime_summary_interval_seconds: u32,
    pub realtime_summary_prompt: String,
    pub default_realtime_summary_prompt: String,
}

impl From<MeetingIntelligenceSettings> for MeetingIntelligenceSettingsView {
    fn from(settings: MeetingIntelligenceSettings) -> Self {
        Self {
            model_mode: settings.model_mode,
            provider: settings.provider,
            model: settings.model,
            api_key: settings.api_key,
            ollama_endpoint: settings.ollama_endpoint,
            custom_openai_base_url: settings.custom_openai_base_url,
            custom_openai_api_key: settings.custom_openai_api_key,
            intelligent_transcript_enabled: settings.intelligent_transcript_enabled,
            intelligent_transcript_prompt: settings.intelligent_transcript_prompt,
            default_intelligent_transcript_prompt: DEFAULT_INTELLIGENT_TRANSCRIPT_PROMPT
                .to_string(),
            realtime_summary_enabled: settings.realtime_summary_enabled,
            realtime_summary_interval_seconds: settings.realtime_summary_interval_seconds,
            realtime_summary_prompt: settings.realtime_summary_prompt,
            default_realtime_summary_prompt: DEFAULT_REALTIME_SUMMARY_PROMPT.to_string(),
        }
    }
}

impl MeetingIntelligenceSettings {
    fn apply_update(&mut self, update: MeetingIntelligenceSettingsUpdate) -> Result<()> {
        validate_model_settings(&update)?;
        let prompt = update.intelligent_transcript_prompt.trim();
        if prompt.is_empty() {
            return Err(anyhow!("Intelligent transcript prompt cannot be empty"));
        }
        if prompt.chars().count() > MAX_PROMPT_CHARS {
            return Err(anyhow!(
                "Intelligent transcript prompt cannot exceed {MAX_PROMPT_CHARS} characters"
            ));
        }
        let realtime_prompt = update.realtime_summary_prompt.trim();
        if realtime_prompt.is_empty() {
            return Err(anyhow!("Realtime summary prompt cannot be empty"));
        }
        if realtime_prompt.chars().count() > MAX_PROMPT_CHARS {
            return Err(anyhow!(
                "Realtime summary prompt cannot exceed {MAX_PROMPT_CHARS} characters"
            ));
        }
        if !(MIN_REALTIME_SUMMARY_INTERVAL_SECONDS..=MAX_REALTIME_SUMMARY_INTERVAL_SECONDS)
            .contains(&update.realtime_summary_interval_seconds)
        {
            return Err(anyhow!(
                "Realtime summary interval must be between {MIN_REALTIME_SUMMARY_INTERVAL_SECONDS} and {MAX_REALTIME_SUMMARY_INTERVAL_SECONDS} seconds"
            ));
        }
        self.model_mode = update.model_mode;
        self.provider = normalize_optional(update.provider);
        self.model = normalize_optional(update.model);
        self.api_key = normalize_optional(update.api_key);
        self.ollama_endpoint = normalize_endpoint(update.ollama_endpoint);
        self.custom_openai_base_url = normalize_endpoint(update.custom_openai_base_url);
        self.custom_openai_api_key = normalize_optional(update.custom_openai_api_key);
        self.intelligent_transcript_enabled = update.intelligent_transcript_enabled;
        self.intelligent_transcript_prompt = prompt.to_string();
        self.realtime_summary_enabled = update.realtime_summary_enabled;
        self.realtime_summary_interval_seconds = update.realtime_summary_interval_seconds;
        self.realtime_summary_prompt = realtime_prompt.to_string();
        Ok(())
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_endpoint(value: Option<String>) -> Option<String> {
    normalize_optional(value).map(|value| value.trim_end_matches('/').to_string())
}

fn validate_http_endpoint(value: &str, label: &str) -> Result<()> {
    if !value.starts_with("http://") && !value.starts_with("https://") {
        return Err(anyhow!("{label} must start with http:// or https://"));
    }
    Ok(())
}

fn validate_model_settings(update: &MeetingIntelligenceSettingsUpdate) -> Result<()> {
    if update.model_mode == MeetingIntelligenceModelMode::FollowSummary {
        return Ok(());
    }
    let provider = update.provider.as_deref().unwrap_or_default().trim();
    let model = update.model.as_deref().unwrap_or_default().trim();
    let parsed = crate::summary::llm_client::LLMProvider::from_str(provider)
        .map_err(|error| anyhow!(error))?;
    if model.is_empty() {
        return Err(anyhow!("Select a model for Meeting Notes"));
    }
    match parsed {
        crate::summary::llm_client::LLMProvider::Ollama => {
            if let Some(endpoint) = update.ollama_endpoint.as_deref() {
                if !endpoint.trim().is_empty() {
                    validate_http_endpoint(endpoint.trim(), "Ollama endpoint")?;
                }
            }
        }
        crate::summary::llm_client::LLMProvider::BuiltInAI => {}
        crate::summary::llm_client::LLMProvider::CustomOpenAI => {
            let endpoint = update
                .custom_openai_base_url
                .as_deref()
                .unwrap_or_default()
                .trim();
            if endpoint.is_empty() {
                return Err(anyhow!("Enter a Custom OpenAI base URL for Meeting Notes"));
            }
            validate_http_endpoint(endpoint, "Custom OpenAI base URL")?;
        }
        _ => {
            if update
                .api_key
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return Err(anyhow!(
                    "Enter an API key for the selected Meeting Notes provider"
                ));
            }
        }
    }
    Ok(())
}

pub fn load_settings<R: Runtime>(app: &AppHandle<R>) -> MeetingIntelligenceSettings {
    let Ok(store) = app.store(STORE_FILE) else {
        return MeetingIntelligenceSettings::default();
    };
    let mut settings: MeetingIntelligenceSettings = store
        .get(STORE_KEY)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    if settings.intelligent_transcript_prompt == LEGACY_INTELLIGENT_TRANSCRIPT_PROMPT {
        settings.intelligent_transcript_prompt = DEFAULT_INTELLIGENT_TRANSCRIPT_PROMPT.to_string();
    }
    settings
}

pub fn save_settings<R: Runtime>(
    app: &AppHandle<R>,
    update: MeetingIntelligenceSettingsUpdate,
) -> Result<MeetingIntelligenceSettings, String> {
    let mut settings = load_settings(app);
    settings
        .apply_update(update)
        .map_err(|error| error.to_string())?;
    let store = app
        .store(STORE_FILE)
        .map_err(|error| format!("Failed to access meeting intelligence settings: {error}"))?;
    store.set(
        STORE_KEY,
        serde_json::to_value(&settings).map_err(|error| {
            format!("Failed to serialize meeting intelligence settings: {error}")
        })?,
    );
    store
        .save()
        .map_err(|error| format!("Failed to save meeting intelligence settings: {error}"))?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_prompt_preserves_roles_and_rejects_subjective_commentary() {
        let prompt = DEFAULT_INTELLIGENT_TRANSCRIPT_PROMPT;
        assert!(prompt.contains("speaker"));
        assert!(prompt.contains("mic"));
        assert!(prompt.contains("不得杜撰"));
        assert!(prompt.contains("主观旁白"));
    }

    #[test]
    fn legacy_default_prompt_has_an_exact_migration_target() {
        assert_ne!(
            LEGACY_INTELLIGENT_TRANSCRIPT_PROMPT,
            DEFAULT_INTELLIGENT_TRANSCRIPT_PROMPT
        );
        assert!(LEGACY_INTELLIGENT_TRANSCRIPT_PROMPT.contains("完整的会议详细记录"));
        assert!(DEFAULT_INTELLIGENT_TRANSCRIPT_PROMPT.contains("一个已经结束的单方发言轮次"));
    }

    #[test]
    fn settings_validate_prompt_length_and_whitespace() {
        let mut settings = MeetingIntelligenceSettings::default();
        assert!(settings
            .apply_update(MeetingIntelligenceSettingsUpdate {
                model_mode: MeetingIntelligenceModelMode::FollowSummary,
                provider: None,
                model: None,
                api_key: None,
                ollama_endpoint: None,
                custom_openai_base_url: None,
                custom_openai_api_key: None,
                intelligent_transcript_enabled: true,
                intelligent_transcript_prompt: "   ".to_string(),
                realtime_summary_enabled: true,
                realtime_summary_interval_seconds: 120,
                realtime_summary_prompt: DEFAULT_REALTIME_SUMMARY_PROMPT.to_string(),
            })
            .is_err());
        settings
            .apply_update(MeetingIntelligenceSettingsUpdate {
                model_mode: MeetingIntelligenceModelMode::FollowSummary,
                provider: None,
                model: None,
                api_key: None,
                ollama_endpoint: None,
                custom_openai_base_url: None,
                custom_openai_api_key: None,
                intelligent_transcript_enabled: false,
                intelligent_transcript_prompt: "  Direct output  ".to_string(),
                realtime_summary_enabled: true,
                realtime_summary_interval_seconds: 120,
                realtime_summary_prompt: DEFAULT_REALTIME_SUMMARY_PROMPT.to_string(),
            })
            .unwrap();
        assert!(!settings.intelligent_transcript_enabled);
        assert_eq!(settings.intelligent_transcript_prompt, "Direct output");
    }

    #[test]
    fn realtime_summary_defaults_to_two_minutes_and_validates_bounds() {
        let settings = MeetingIntelligenceSettings::default();
        assert_eq!(settings.realtime_summary_interval_seconds, 120);
        assert!(settings.realtime_summary_enabled);

        let mut settings = MeetingIntelligenceSettings::default();
        let result = settings.apply_update(MeetingIntelligenceSettingsUpdate {
            model_mode: MeetingIntelligenceModelMode::FollowSummary,
            provider: None,
            model: None,
            api_key: None,
            ollama_endpoint: None,
            custom_openai_base_url: None,
            custom_openai_api_key: None,
            intelligent_transcript_enabled: true,
            intelligent_transcript_prompt: DEFAULT_INTELLIGENT_TRANSCRIPT_PROMPT.to_string(),
            realtime_summary_enabled: true,
            realtime_summary_interval_seconds: 30,
            realtime_summary_prompt: DEFAULT_REALTIME_SUMMARY_PROMPT.to_string(),
        });
        assert!(result.is_err());
    }

    #[test]
    fn independent_model_requires_provider_specific_connection_settings() {
        let mut settings = MeetingIntelligenceSettings::default();
        let mut update = MeetingIntelligenceSettingsUpdate {
            model_mode: MeetingIntelligenceModelMode::Custom,
            provider: Some("openai".to_string()),
            model: Some("gpt-4.1-mini".to_string()),
            api_key: None,
            ollama_endpoint: None,
            custom_openai_base_url: None,
            custom_openai_api_key: None,
            intelligent_transcript_enabled: true,
            intelligent_transcript_prompt: DEFAULT_INTELLIGENT_TRANSCRIPT_PROMPT.to_string(),
            realtime_summary_enabled: true,
            realtime_summary_interval_seconds: 120,
            realtime_summary_prompt: DEFAULT_REALTIME_SUMMARY_PROMPT.to_string(),
        };
        assert!(settings.apply_update(update.clone()).is_err());

        update.provider = Some("custom-openai".to_string());
        update.custom_openai_base_url = Some("http://localhost:8000/v1/".to_string());
        settings.apply_update(update).unwrap();
        assert_eq!(
            settings.custom_openai_base_url.as_deref(),
            Some("http://localhost:8000/v1")
        );
    }
}
