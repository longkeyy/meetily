use super::{
    GenerateIntelligentTranscriptRequest, IntelligenceTranscriptInput,
    IntelligentTranscriptDocument, IntelligentTranscriptResponse,
};
use crate::database::repositories::{meeting::MeetingsRepository, setting::SettingsRepository};
use crate::summary::llm_client::{generate_summary, LLMProvider};
use crate::summary::processor::clean_llm_markdown_output;
use reqwest::Client;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Runtime};
use tokio_util::sync::CancellationToken;

const DOCUMENT_FILE: &str = "intelligent_transcript.json";
const MAX_INPUT_SEGMENTS: usize = 2_000;
const MAX_CONTEXT_CHARS: usize = 10_000;
const MAX_OUTPUT_TOKENS: u32 = 3_000;

static BACKGROUND_GENERATION_ID: AtomicU64 = AtomicU64::new(0);
static ACTIVE_BACKGROUND_GENERATION: Mutex<Option<(u64, CancellationToken)>> = Mutex::new(None);

pub(crate) struct LlmRuntimeConfig {
    pub(crate) provider_name: String,
    pub(crate) provider: LLMProvider,
    pub(crate) model: String,
    pub(crate) api_key: String,
    pub(crate) ollama_endpoint: Option<String>,
    pub(crate) custom_openai_endpoint: Option<String>,
    pub(crate) max_tokens: Option<u32>,
    pub(crate) temperature: Option<f32>,
    pub(crate) top_p: Option<f32>,
    pub(crate) app_data_dir: Option<PathBuf>,
}

pub async fn generate_for_live_recording<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    request: GenerateIntelligentTranscriptRequest,
    system_prompt: &str,
) -> Result<IntelligentTranscriptResponse, String> {
    if request.request_id.trim().is_empty() {
        return Err("Request ID cannot be empty".to_string());
    }
    if request.transcripts.is_empty() {
        return Err("No transcript is available for intelligent recording".to_string());
    }
    if request.transcripts.len() > MAX_INPUT_SEGMENTS {
        return Err(format!(
            "Too many transcript segments: {} (maximum {MAX_INPUT_SEGMENTS})",
            request.transcripts.len()
        ));
    }
    let folder = PathBuf::from(request.meeting_folder.trim());
    if !folder.is_dir() {
        return Err("Meeting folder does not exist".to_string());
    }
    let document = generate_document(
        app,
        pool,
        &folder,
        &request.transcripts,
        request.force_full,
        system_prompt,
    )
    .await?;
    Ok(IntelligentTranscriptResponse {
        request_id: request.request_id,
        document,
    })
}

pub async fn regenerate_for_meeting<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    meeting_id: &str,
    system_prompt: &str,
) -> Result<IntelligentTranscriptDocument, String> {
    let meeting = MeetingsRepository::get_meeting_metadata(pool, meeting_id)
        .await
        .map_err(|error| format!("Failed to load meeting: {error}"))?
        .ok_or_else(|| "Meeting not found".to_string())?;
    let folder = meeting
        .folder_path
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .ok_or_else(|| "Meeting folder is unavailable".to_string())?;
    let rows = sqlx::query_as::<_, (String, Option<f64>, Option<f64>, Option<String>)>(
        "SELECT transcript, audio_start_time, audio_end_time, speaker FROM transcripts \
         WHERE meeting_id = ? ORDER BY audio_start_time ASC, timestamp ASC",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("Failed to load meeting transcripts: {error}"))?;
    let transcripts = rows
        .into_iter()
        .enumerate()
        .map(
            |(index, (text, start, end, source))| IntelligenceTranscriptInput {
                sequence_id: Some(index as u64 + 1),
                source,
                text,
                audio_start_time: start,
                audio_end_time: end,
            },
        )
        .collect::<Vec<_>>();
    if transcripts.is_empty() {
        return Err("Meeting has no transcript".to_string());
    }
    generate_document(app, pool, &folder, &transcripts, true, system_prompt).await
}

pub async fn load_for_meeting(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<Option<IntelligentTranscriptDocument>, String> {
    let meeting = MeetingsRepository::get_meeting_metadata(pool, meeting_id)
        .await
        .map_err(|error| format!("Failed to load meeting: {error}"))?
        .ok_or_else(|| "Meeting not found".to_string())?;
    let Some(folder) = meeting.folder_path.map(PathBuf::from) else {
        return Ok(None);
    };
    read_document(&folder)
}

async fn generate_document<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    folder: &Path,
    transcripts: &[IntelligenceTranscriptInput],
    force_full: bool,
    system_prompt: &str,
) -> Result<IntelligentTranscriptDocument, String> {
    let previous = if force_full {
        None
    } else {
        read_document(folder)?
    };
    let covered_until = previous
        .as_ref()
        .map(|document| document.covered_until)
        .unwrap_or(0.0);
    let selected = if force_full {
        transcripts.iter().collect::<Vec<_>>()
    } else {
        transcripts
            .iter()
            .filter(|segment| segment.audio_end_time.unwrap_or(0.0) > covered_until)
            .collect::<Vec<_>>()
    };
    if selected.is_empty() {
        return previous.ok_or_else(|| "No new transcript is available".to_string());
    }

    let mut markdown = previous
        .as_ref()
        .map(|document| document.markdown.clone())
        .unwrap_or_default();
    let config = load_runtime_config(app, pool).await?;
    let client = Client::new();
    let (generation_id, cancellation_token) = register_background_generation();
    let generation_result = async {
        for chunk in chunk_transcripts(&selected, MAX_CONTEXT_CHARS) {
            let context = build_transcript_context(&chunk, MAX_CONTEXT_CHARS);
            let previous_markdown = if markdown.is_empty() {
                "(none)"
            } else {
                markdown.as_str()
            };
            let user_prompt = format!(
                "Existing detailed record:\n{previous_markdown}\n\nNew raw transcript:\n{context}\n\nReturn the complete updated detailed record only."
            );
            let output = generate_summary(
                &client,
                &config.provider,
                &config.model,
                &config.api_key,
                system_prompt,
                &user_prompt,
                config.ollama_endpoint.as_deref(),
                config.custom_openai_endpoint.as_deref(),
                config.max_tokens.or(Some(MAX_OUTPUT_TOKENS)),
                config.temperature.or(Some(0.2)),
                config.top_p.or(Some(0.9)),
                config.app_data_dir.as_ref(),
                Some(&cancellation_token),
            )
            .await?;
            markdown = clean_llm_markdown_output(&output);
            if markdown.is_empty() {
                return Err("The model returned an empty intelligent transcript".to_string());
            }
        }
        Ok::<(), String>(())
    }
    .await;
    cleanup_background_generation(generation_id);
    generation_result?;
    if markdown.is_empty() {
        return Err("The model returned an empty intelligent transcript".to_string());
    }
    let document = IntelligentTranscriptDocument {
        version: 1,
        markdown,
        covered_until: transcripts
            .iter()
            .filter_map(|segment| segment.audio_end_time)
            .fold(0.0, f64::max),
        source_revision: transcripts
            .iter()
            .filter_map(|segment| segment.sequence_id)
            .max()
            .unwrap_or(transcripts.len() as u64),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    write_document(folder, &document)?;
    Ok(document)
}

pub(crate) fn register_background_generation() -> (u64, CancellationToken) {
    let generation_id = BACKGROUND_GENERATION_ID.fetch_add(1, Ordering::Relaxed) + 1;
    let token = CancellationToken::new();
    if let Ok(mut active) = ACTIVE_BACKGROUND_GENERATION.lock() {
        if let Some((_, previous)) = active.replace((generation_id, token.clone())) {
            previous.cancel();
        }
    }
    (generation_id, token)
}

pub(crate) fn cleanup_background_generation(generation_id: u64) {
    if let Ok(mut active) = ACTIVE_BACKGROUND_GENERATION.lock() {
        if active
            .as_ref()
            .is_some_and(|(active_id, _)| *active_id == generation_id)
        {
            active.take();
        }
    }
}

pub fn cancel_background_generation() -> bool {
    let Ok(mut active) = ACTIVE_BACKGROUND_GENERATION.lock() else {
        return false;
    };
    if let Some((_, token)) = active.take() {
        token.cancel();
        true
    } else {
        false
    }
}

pub(crate) async fn load_runtime_config<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
) -> Result<LlmRuntimeConfig, String> {
    let intelligence_settings = super::settings::load_settings(app);
    if intelligence_settings.model_mode == super::settings::MeetingIntelligenceModelMode::Custom {
        let provider_name = intelligence_settings
            .provider
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_string();
        let provider = LLMProvider::from_str(&provider_name)?;
        let model = intelligence_settings
            .model
            .filter(|model| !model.trim().is_empty())
            .ok_or_else(|| "Select a model for Meeting Notes".to_string())?;
        let api_key = match &provider {
            LLMProvider::Ollama | LLMProvider::BuiltInAI => String::new(),
            LLMProvider::CustomOpenAI => intelligence_settings
                .custom_openai_api_key
                .unwrap_or_default(),
            _ => intelligence_settings
                .api_key
                .filter(|key| !key.trim().is_empty())
                .ok_or_else(|| "API key not found for Meeting Notes provider".to_string())?,
        };
        return Ok(LlmRuntimeConfig {
            provider_name,
            provider,
            model,
            api_key,
            ollama_endpoint: intelligence_settings.ollama_endpoint,
            custom_openai_endpoint: intelligence_settings.custom_openai_base_url,
            max_tokens: None,
            temperature: None,
            top_p: None,
            app_data_dir: app.path().app_data_dir().ok(),
        });
    }

    let setting = SettingsRepository::get_model_config(pool)
        .await
        .map_err(|error| format!("Failed to read summary model configuration: {error}"))?
        .ok_or_else(|| {
            "Configure a summary model before enabling intelligent recording".to_string()
        })?;
    let provider_name = setting.provider.clone();
    let provider = LLMProvider::from_str(&provider_name)?;
    let mut model = setting.model;
    let mut api_key = String::new();
    let mut custom_openai_endpoint = None;
    let mut max_tokens = None;
    let mut temperature = None;
    let mut top_p = None;

    if provider == LLMProvider::CustomOpenAI {
        let custom = SettingsRepository::get_custom_openai_config(pool)
            .await
            .map_err(|error| format!("Failed to read Custom OpenAI configuration: {error}"))?
            .ok_or_else(|| "Custom OpenAI is selected but is not configured".to_string())?;
        model = custom.model;
        api_key = custom.api_key.unwrap_or_default();
        custom_openai_endpoint = Some(custom.endpoint);
        max_tokens = custom.max_tokens.map(|value| value as u32);
        temperature = custom.temperature;
        top_p = custom.top_p;
    } else if !matches!(provider, LLMProvider::Ollama | LLMProvider::BuiltInAI) {
        api_key = SettingsRepository::get_api_key(pool, &setting.provider)
            .await
            .map_err(|error| format!("Failed to read API key: {error}"))?
            .filter(|key| !key.trim().is_empty())
            .ok_or_else(|| format!("API key not found for {}", setting.provider))?;
    }

    Ok(LlmRuntimeConfig {
        provider_name,
        provider,
        model,
        api_key,
        ollama_endpoint: setting.ollama_endpoint,
        custom_openai_endpoint,
        max_tokens,
        temperature,
        top_p,
        app_data_dir: app.path().app_data_dir().ok(),
    })
}

fn build_transcript_context(
    transcripts: &[&IntelligenceTranscriptInput],
    max_chars: usize,
) -> String {
    let mut lines = Vec::new();
    let mut used = 0;
    for segment in transcripts.iter().rev() {
        let text = segment.text.trim();
        if text.is_empty() {
            continue;
        }
        let source = match segment.source.as_deref() {
            Some("mic") | Some("microphone") => "mic",
            Some("system") | Some("speaker") | Some("systemAudio") => "speaker",
            _ => "speaker",
        };
        let seconds = segment.audio_start_time.unwrap_or(0.0).max(0.0) as u64;
        let line = format!("[{:02}:{:02}] {source}: {text}", seconds / 60, seconds % 60);
        if used + line.chars().count() > max_chars && !lines.is_empty() {
            break;
        }
        used += line.chars().count();
        lines.push(line);
    }
    lines.reverse();
    lines.join("\n")
}

fn chunk_transcripts<'a>(
    transcripts: &[&'a IntelligenceTranscriptInput],
    max_chars: usize,
) -> Vec<Vec<&'a IntelligenceTranscriptInput>> {
    let mut chunks = Vec::new();
    let mut current = Vec::new();
    let mut current_chars = 0;
    for transcript in transcripts {
        let estimated_chars = transcript.text.chars().count() + 32;
        if !current.is_empty() && current_chars + estimated_chars > max_chars {
            chunks.push(current);
            current = Vec::new();
            current_chars = 0;
        }
        current.push(*transcript);
        current_chars += estimated_chars;
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn read_document(folder: &Path) -> Result<Option<IntelligentTranscriptDocument>, String> {
    let path = folder.join(DOCUMENT_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read intelligent transcript: {error}"))?;
    serde_json::from_str(&content)
        .map(Some)
        .map_err(|error| format!("Failed to parse intelligent transcript: {error}"))
}

fn write_document(folder: &Path, document: &IntelligentTranscriptDocument) -> Result<(), String> {
    let path = folder.join(DOCUMENT_FILE);
    let temp_path = folder.join(format!(".{DOCUMENT_FILE}.tmp"));
    let json = serde_json::to_string_pretty(document)
        .map_err(|error| format!("Failed to serialize intelligent transcript: {error}"))?;
    std::fs::write(&temp_path, json)
        .map_err(|error| format!("Failed to write intelligent transcript: {error}"))?;
    std::fs::rename(&temp_path, &path)
        .map_err(|error| format!("Failed to finalize intelligent transcript: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_uses_stable_speaker_and_mic_labels() {
        let entries = [
            IntelligenceTranscriptInput {
                sequence_id: Some(1),
                source: Some("system".to_string()),
                text: "介绍一下自动化经验".to_string(),
                audio_start_time: Some(65.0),
                audio_end_time: Some(68.0),
            },
            IntelligenceTranscriptInput {
                sequence_id: Some(2),
                source: Some("mic".to_string()),
                text: "使用 API Fox".to_string(),
                audio_start_time: Some(69.0),
                audio_end_time: Some(72.0),
            },
        ];
        let refs = entries.iter().collect::<Vec<_>>();
        let context = build_transcript_context(&refs, 1_000);
        assert!(context.contains("[01:05] speaker:"));
        assert!(context.contains("[01:09] mic:"));
    }

    #[test]
    fn document_round_trip_is_atomic_and_backward_independent() {
        let folder = tempfile::tempdir().unwrap();
        let document = IntelligentTranscriptDocument {
            version: 1,
            markdown: "speaker 提问，mic 回答。".to_string(),
            covered_until: 12.0,
            source_revision: 3,
            updated_at: "2026-07-30T00:00:00Z".to_string(),
        };
        write_document(folder.path(), &document).unwrap();
        let restored = read_document(folder.path()).unwrap().unwrap();
        assert_eq!(restored.markdown, document.markdown);
        assert!(!folder.path().join(format!(".{DOCUMENT_FILE}.tmp")).exists());
    }

    #[test]
    fn long_transcripts_are_chunked_without_dropping_segments() {
        let entries = (0..5)
            .map(|index| IntelligenceTranscriptInput {
                sequence_id: Some(index + 1),
                source: Some("system".to_string()),
                text: "x".repeat(40),
                audio_start_time: Some(index as f64),
                audio_end_time: Some(index as f64 + 1.0),
            })
            .collect::<Vec<_>>();
        let refs = entries.iter().collect::<Vec<_>>();
        let chunks = chunk_transcripts(&refs, 100);
        assert!(chunks.len() > 1);
        assert_eq!(chunks.iter().map(Vec::len).sum::<usize>(), entries.len());
    }
}
