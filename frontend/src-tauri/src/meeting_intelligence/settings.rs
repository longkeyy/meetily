use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "meeting_intelligence_settings.json";
const STORE_KEY: &str = "settings";
const MAX_PROMPT_CHARS: usize = 8_000;

pub const DEFAULT_INTELLIGENT_TRANSCRIPT_PROMPT: &str = r#"你是会议智能记录助手。你永远不回应会议参与者，只整理输入的原始转录，并直接输出完整的会议详细记录。

要求：
1. 按会议实际发生顺序，用连贯、专业、易读的叙述记录讨论流程。
2. 发言者只能使用 speaker 和 mic 两个名称，必须保留谁提问、谁回答、谁补充或确认了什么。
3. 去除“呃”“嗯”“你知道的”等填充词，以及无意义重复和口吃。
4. 识别说话人的自我更正，只保留最终表达的意图。
5. 将口述的列表、步骤和要点整理为清晰的 Markdown，但不要把详细流程压缩成只有结论的概要。
6. 保留事实、数字、专有名词、问题、回答、决定和未解决事项，不得杜撰。
7. 不添加讽刺、情绪化评价、能力判断或类似“这波”“致命伤”的主观旁白；只有参与者明确表达的评价才能记录，并注明是谁表达的。
8. 不输出“会议详细”等标题，不解释处理过程，直接输出整理后的详细记录。"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MeetingIntelligenceSettings {
    pub intelligent_transcript_enabled: bool,
    pub intelligent_transcript_prompt: String,
}

impl Default for MeetingIntelligenceSettings {
    fn default() -> Self {
        Self {
            intelligent_transcript_enabled: true,
            intelligent_transcript_prompt: DEFAULT_INTELLIGENT_TRANSCRIPT_PROMPT.to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingIntelligenceSettingsUpdate {
    pub intelligent_transcript_enabled: bool,
    pub intelligent_transcript_prompt: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingIntelligenceSettingsView {
    pub intelligent_transcript_enabled: bool,
    pub intelligent_transcript_prompt: String,
    pub default_intelligent_transcript_prompt: String,
}

impl From<MeetingIntelligenceSettings> for MeetingIntelligenceSettingsView {
    fn from(settings: MeetingIntelligenceSettings) -> Self {
        Self {
            intelligent_transcript_enabled: settings.intelligent_transcript_enabled,
            intelligent_transcript_prompt: settings.intelligent_transcript_prompt,
            default_intelligent_transcript_prompt: DEFAULT_INTELLIGENT_TRANSCRIPT_PROMPT
                .to_string(),
        }
    }
}

impl MeetingIntelligenceSettings {
    fn apply_update(&mut self, update: MeetingIntelligenceSettingsUpdate) -> Result<()> {
        let prompt = update.intelligent_transcript_prompt.trim();
        if prompt.is_empty() {
            return Err(anyhow!("Intelligent transcript prompt cannot be empty"));
        }
        if prompt.chars().count() > MAX_PROMPT_CHARS {
            return Err(anyhow!(
                "Intelligent transcript prompt cannot exceed {MAX_PROMPT_CHARS} characters"
            ));
        }
        self.intelligent_transcript_enabled = update.intelligent_transcript_enabled;
        self.intelligent_transcript_prompt = prompt.to_string();
        Ok(())
    }
}

pub fn load_settings<R: Runtime>(app: &AppHandle<R>) -> MeetingIntelligenceSettings {
    let Ok(store) = app.store(STORE_FILE) else {
        return MeetingIntelligenceSettings::default();
    };
    store
        .get(STORE_KEY)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default()
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
    fn settings_validate_prompt_length_and_whitespace() {
        let mut settings = MeetingIntelligenceSettings::default();
        assert!(settings
            .apply_update(MeetingIntelligenceSettingsUpdate {
                intelligent_transcript_enabled: true,
                intelligent_transcript_prompt: "   ".to_string(),
            })
            .is_err());
        settings
            .apply_update(MeetingIntelligenceSettingsUpdate {
                intelligent_transcript_enabled: false,
                intelligent_transcript_prompt: "  Direct output  ".to_string(),
            })
            .unwrap();
        assert!(!settings.intelligent_transcript_enabled);
        assert_eq!(settings.intelligent_transcript_prompt, "Direct output");
    }
}
