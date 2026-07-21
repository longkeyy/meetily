use super::{
    profiles::profile_definition, AssistantSuggestionRequest, AssistantSuggestionResponse,
    AssistantTranscript, SuggestionTrigger,
};
use crate::database::repositories::setting::SettingsRepository;
use crate::summary::llm_client::{generate_summary, LLMProvider};
use crate::summary::processor::clean_llm_markdown_output;
use once_cell::sync::Lazy;
use reqwest::Client;
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Runtime};
use tokio_util::sync::CancellationToken;

const MAX_CONTEXT_CHARS: usize = 12_000;
const MAX_TRANSCRIPTS: usize = 200;
const SUGGESTION_MAX_TOKENS: u32 = 160;

static ACTIVE_GENERATION: Lazy<Mutex<Option<(String, CancellationToken)>>> =
    Lazy::new(|| Mutex::new(None));

struct LlmRuntimeConfig {
    provider: LLMProvider,
    model: String,
    api_key: String,
    ollama_endpoint: Option<String>,
    custom_openai_endpoint: Option<String>,
    app_data_dir: Option<PathBuf>,
}

pub async fn generate_suggestion<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    request: AssistantSuggestionRequest,
) -> Result<AssistantSuggestionResponse, String> {
    validate_request(&request)?;
    let context = build_conversation_context(
        &request.transcripts,
        request.focus_start_time,
        MAX_CONTEXT_CHARS,
    );
    if context.trim().is_empty() {
        return Err("No transcript context is available for a suggestion".to_string());
    }

    let cancellation_token = register_generation(&request.request_id);
    let result = async {
        let config = load_llm_config(app, pool).await?;
        let profile = profile_definition(request.profile);
        let user_prompt = build_user_prompt(request.trigger, &context);
        let client = Client::new();

        generate_summary(
            &client,
            &config.provider,
            &config.model,
            &config.api_key,
            profile.system_prompt,
            &user_prompt,
            config.ollama_endpoint.as_deref(),
            config.custom_openai_endpoint.as_deref(),
            Some(SUGGESTION_MAX_TOKENS),
            Some(0.35),
            Some(0.9),
            config.app_data_dir.as_ref(),
            Some(&cancellation_token),
        )
        .await
    }
    .await;

    cleanup_generation(&request.request_id);
    let suggestion = clean_llm_markdown_output(&result?);
    if suggestion.is_empty() {
        return Err("The assistant returned an empty suggestion".to_string());
    }

    Ok(AssistantSuggestionResponse {
        request_id: request.request_id,
        profile: request.profile,
        trigger: request.trigger,
        suggestion,
    })
}

pub fn cancel_generation() -> bool {
    let Ok(mut active) = ACTIVE_GENERATION.lock() else {
        return false;
    };
    if let Some((_, token)) = active.take() {
        token.cancel();
        true
    } else {
        false
    }
}

fn register_generation(request_id: &str) -> CancellationToken {
    let token = CancellationToken::new();
    if let Ok(mut active) = ACTIVE_GENERATION.lock() {
        if let Some((_, previous)) = active.replace((request_id.to_string(), token.clone())) {
            previous.cancel();
        }
    }
    token
}

fn cleanup_generation(request_id: &str) {
    if let Ok(mut active) = ACTIVE_GENERATION.lock() {
        if active
            .as_ref()
            .is_some_and(|(active_request_id, _)| active_request_id == request_id)
        {
            active.take();
        }
    }
}

async fn load_llm_config<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
) -> Result<LlmRuntimeConfig, String> {
    let setting = SettingsRepository::get_model_config(pool)
        .await
        .map_err(|error| format!("Failed to read summary model configuration: {error}"))?
        .ok_or_else(|| "Configure a summary model before enabling the assistant".to_string())?;
    let provider = LLMProvider::from_str(&setting.provider)?;

    let mut model = setting.model;
    let mut custom_openai_endpoint = None;
    let mut api_key = String::new();

    if provider == LLMProvider::CustomOpenAI {
        let custom = SettingsRepository::get_custom_openai_config(pool)
            .await
            .map_err(|error| format!("Failed to read custom OpenAI configuration: {error}"))?
            .ok_or_else(|| "Custom OpenAI is selected but is not configured".to_string())?;
        model = custom.model;
        api_key = custom.api_key.unwrap_or_default();
        custom_openai_endpoint = Some(custom.endpoint);
    } else if !matches!(&provider, LLMProvider::Ollama | LLMProvider::BuiltInAI) {
        api_key = SettingsRepository::get_api_key(pool, &setting.provider)
            .await
            .map_err(|error| format!("Failed to read the summary API key: {error}"))?
            .filter(|key| !key.trim().is_empty())
            .ok_or_else(|| format!("API key not found for {}", setting.provider))?;
    }

    Ok(LlmRuntimeConfig {
        provider,
        model,
        api_key,
        ollama_endpoint: setting.ollama_endpoint,
        custom_openai_endpoint,
        app_data_dir: app.path().app_data_dir().ok(),
    })
}

fn validate_request(request: &AssistantSuggestionRequest) -> Result<(), String> {
    if request.request_id.trim().is_empty() {
        return Err("Assistant request ID cannot be empty".to_string());
    }
    if request.transcripts.is_empty() {
        return Err("At least one transcript is required".to_string());
    }
    if request.transcripts.len() > MAX_TRANSCRIPTS {
        return Err(format!(
            "Too many transcript segments: {} (maximum {})",
            request.transcripts.len(),
            MAX_TRANSCRIPTS
        ));
    }
    Ok(())
}

fn build_user_prompt(trigger: SuggestionTrigger, context: &str) -> String {
    let focus = match trigger {
        SuggestionTrigger::Periodic => {
            "The interviewer is still speaking. Respond to the most recent FOCUS portion without assuming the turn is complete."
        }
        SuggestionTrigger::TurnEnd => {
            "The interviewer has finished this turn. Recommend the candidate's best next response."
        }
        SuggestionTrigger::Manual => {
            "The candidate requested a refreshed suggestion based on the latest conversation."
        }
    };
    format!("{focus}\n\nConversation transcript:\n{context}")
}

fn build_conversation_context(
    transcripts: &[AssistantTranscript],
    focus_start_time: Option<f64>,
    max_chars: usize,
) -> String {
    let mut lines = Vec::new();
    let mut char_count = 0;

    for transcript in transcripts.iter().rev() {
        let text = transcript.text.trim();
        if text.is_empty() {
            continue;
        }
        let speaker = match transcript.source {
            crate::audio::recording_state::TranscriptSource::Microphone => "MIC",
            crate::audio::recording_state::TranscriptSource::SystemAudio => "SPEAKER",
        };
        let focus = focus_start_time
            .is_some_and(|start| transcript.audio_end_time >= start)
            .then_some(" [FOCUS]")
            .unwrap_or_default();
        let line = format!(
            "[{:.1}-{:.1}] {speaker}{focus}: {text}",
            transcript.audio_start_time, transcript.audio_end_time
        );
        let line_chars = line.chars().count() + 1;
        if !lines.is_empty() && char_count + line_chars > max_chars {
            break;
        }
        char_count += line_chars;
        lines.push(line);
    }

    lines.reverse();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::recording_state::TranscriptSource;

    fn transcript(
        source: TranscriptSource,
        text: &str,
        start: f64,
        end: f64,
    ) -> AssistantTranscript {
        AssistantTranscript {
            source,
            text: text.to_string(),
            audio_start_time: start,
            audio_end_time: end,
        }
    }

    #[test]
    fn context_uses_role_labels_and_marks_the_focus_window() {
        let context = build_conversation_context(
            &[
                transcript(
                    TranscriptSource::Microphone,
                    "I work on search systems",
                    1.0,
                    3.0,
                ),
                transcript(
                    TranscriptSource::SystemAudio,
                    "How did you measure quality?",
                    35.0,
                    38.0,
                ),
            ],
            Some(30.0),
            MAX_CONTEXT_CHARS,
        );

        assert!(context.contains("MIC: I work on search systems"));
        assert!(context.contains("SPEAKER [FOCUS]: How did you measure quality?"));
    }

    #[test]
    fn context_keeps_the_newest_segments_within_the_limit() {
        let context = build_conversation_context(
            &[
                transcript(TranscriptSource::Microphone, "old response", 1.0, 2.0),
                transcript(TranscriptSource::SystemAudio, "latest question", 3.0, 4.0),
            ],
            None,
            45,
        );

        assert!(!context.contains("old response"));
        assert!(context.contains("latest question"));
    }
}
