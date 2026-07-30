use super::{
    GenerateIntelligentTranscriptRequest, IntelligenceTranscriptInput,
    IntelligentTranscriptDocument, IntelligentTranscriptModelInfo, IntelligentTranscriptResponse,
    IntelligentTranscriptTurn,
};
use crate::database::repositories::{meeting::MeetingsRepository, setting::SettingsRepository};
use crate::summary::llm_client::{generate_summary, LLMProvider};
use crate::summary::processor::clean_llm_markdown_output;
use reqwest::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Runtime};
use tokio_util::sync::CancellationToken;

const DOCUMENT_FILE: &str = "intelligent_transcript.jsonl";
const LEGACY_DOCUMENT_FILE: &str = "intelligent_transcript.json";
const MAX_INPUT_SEGMENTS: usize = 2_000;
const MAX_CONTEXT_CHARS: usize = 10_000;
const MAX_OUTPUT_TOKENS: u32 = 1_600;

// Refined turns must be committed in source order. This lock is intentionally
// separate from realtime summary cancellation so neither feature interrupts the other.
static TURN_GENERATION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static BACKGROUND_GENERATION_ID: AtomicU64 = AtomicU64::new(0);
static ACTIVE_BACKGROUND_GENERATION: Mutex<Option<(u64, CancellationToken)>> = Mutex::new(None);

struct TranscriptTurn<'a> {
    source: &'static str,
    start_seconds: f64,
    end_seconds: f64,
    source_revision_start: u64,
    source_revision_end: u64,
    transcripts: Vec<&'a IntelligenceTranscriptInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyIntelligentTranscriptDocument {
    markdown: String,
    covered_until: f64,
    source_revision: u64,
    updated_at: String,
}

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
    validate_request(&request)?;
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
    let (folder, transcripts) = load_meeting_transcripts(pool, meeting_id).await?;
    generate_document(app, pool, &folder, &transcripts, true, true, system_prompt).await
}

pub async fn finalize_for_meeting<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    meeting_id: &str,
    system_prompt: &str,
) -> Result<IntelligentTranscriptDocument, String> {
    let (folder, transcripts) = load_meeting_transcripts(pool, meeting_id).await?;
    generate_document(app, pool, &folder, &transcripts, false, true, system_prompt).await
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

fn validate_request(request: &GenerateIntelligentTranscriptRequest) -> Result<(), String> {
    if request.request_id.trim().is_empty() {
        return Err("Request ID cannot be empty".to_string());
    }
    if request.transcripts.is_empty() {
        return Err("No transcript is available for refined recording".to_string());
    }
    if request.transcripts.len() > MAX_INPUT_SEGMENTS {
        return Err(format!(
            "Too many transcript segments: {} (maximum {MAX_INPUT_SEGMENTS})",
            request.transcripts.len()
        ));
    }
    Ok(())
}

async fn load_meeting_transcripts(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<(PathBuf, Vec<IntelligenceTranscriptInput>), String> {
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
    Ok((folder, transcripts))
}

async fn generate_document<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    folder: &Path,
    transcripts: &[IntelligenceTranscriptInput],
    regenerate: bool,
    include_active_turn: bool,
    system_prompt: &str,
) -> Result<IntelligentTranscriptDocument, String> {
    let _generation_guard = TURN_GENERATION_LOCK.lock().await;
    let previous = if regenerate {
        None
    } else {
        read_document(folder)?
    };
    let previous_revision = previous
        .as_ref()
        .map(|document| document.source_revision)
        .unwrap_or(0);
    let previous_end = previous
        .as_ref()
        .map(|document| document.covered_until)
        .unwrap_or(0.0);
    let turns = group_transcripts(transcripts, include_active_turn);
    let selected = turns
        .into_iter()
        .filter(|turn| is_new_turn(turn, regenerate, previous_revision, previous_end))
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return previous.ok_or_else(|| "No completed speaker turn is available yet".to_string());
    }

    let config = load_runtime_config(app, pool).await?;
    let model = IntelligentTranscriptModelInfo {
        provider: config.provider_name.clone(),
        model: config.model.clone(),
    };
    let prompt_hash = prompt_hash(system_prompt);
    let client = Client::new();
    let mut generated = Vec::with_capacity(selected.len());
    for turn in selected {
        let raw_text = turn
            .transcripts
            .iter()
            .filter_map(|segment| {
                let text = segment.text.trim();
                (!text.is_empty()).then_some(text)
            })
            .collect::<Vec<_>>()
            .join(" ");
        if raw_text.is_empty() {
            continue;
        }
        let content =
            generate_turn_content(&client, &config, turn.source, &raw_text, system_prompt).await?;
        generated.push(IntelligentTranscriptTurn {
            schema_version: 1,
            turn_id: uuid::Uuid::new_v4().to_string(),
            source: turn.source.to_string(),
            start_seconds: turn.start_seconds,
            end_seconds: turn.end_seconds,
            source_revision_start: turn.source_revision_start,
            source_revision_end: turn.source_revision_end,
            raw_text,
            content,
            created_at: chrono::Utc::now().to_rfc3339(),
            model: model.clone(),
            prompt_hash: prompt_hash.clone(),
        });
    }
    if generated.is_empty() {
        return previous.ok_or_else(|| "No completed speaker turn contains text".to_string());
    }

    let mut all_turns = previous
        .as_ref()
        .map(|document| document.turns.clone())
        .unwrap_or_default();
    all_turns.extend(generated.iter().cloned());
    if regenerate || !folder.join(DOCUMENT_FILE).exists() {
        rewrite_turns(folder, &all_turns)?;
    } else {
        append_turns(folder, &generated)?;
    }
    Ok(document_from_turns(all_turns))
}

async fn generate_turn_content(
    client: &Client,
    config: &LlmRuntimeConfig,
    source: &str,
    raw_text: &str,
    system_prompt: &str,
) -> Result<String, String> {
    let mut content = String::new();
    for chunk in chunk_text(raw_text, MAX_CONTEXT_CHARS) {
        let draft = if content.is_empty() {
            "(none)"
        } else {
            content.as_str()
        };
        let user_prompt = format!(
            "Source is fixed by the application as {source}. Never change or output the source label.\n\nCurrent refined draft (use only when this turn was split for input size):\n{draft}\n\nRaw text for this completed turn:\n{chunk}\n\nReturn only the complete refined text for this turn. Do not add a speaker label or heading."
        );
        let output = generate_summary(
            client,
            &config.provider,
            &config.model,
            &config.api_key,
            system_prompt,
            &user_prompt,
            config.ollama_endpoint.as_deref(),
            config.custom_openai_endpoint.as_deref(),
            config.max_tokens.or(Some(MAX_OUTPUT_TOKENS)),
            config.temperature.or(Some(0.15)),
            config.top_p.or(Some(0.9)),
            config.app_data_dir.as_ref(),
            None,
        )
        .await?;
        content = clean_llm_markdown_output(&output).trim().to_string();
        if content.is_empty() {
            return Err("The model returned an empty refined turn".to_string());
        }
    }
    Ok(content)
}

fn source_label(source: Option<&str>) -> &'static str {
    match source {
        Some("mic") | Some("microphone") => "mic",
        _ => "speaker",
    }
}

fn group_transcripts<'a>(
    transcripts: &'a [IntelligenceTranscriptInput],
    include_active_turn: bool,
) -> Vec<TranscriptTurn<'a>> {
    let mut turns: Vec<TranscriptTurn<'a>> = Vec::new();
    for (index, transcript) in transcripts.iter().enumerate() {
        if transcript.text.trim().is_empty() {
            continue;
        }
        let source = source_label(transcript.source.as_deref());
        let revision = transcript.sequence_id.unwrap_or(index as u64 + 1);
        let start = transcript.audio_start_time.unwrap_or(0.0).max(0.0);
        let end = transcript.audio_end_time.unwrap_or(start).max(start);
        if let Some(current) = turns.last_mut().filter(|turn| turn.source == source) {
            current.end_seconds = current.end_seconds.max(end);
            current.source_revision_end = current.source_revision_end.max(revision);
            current.transcripts.push(transcript);
        } else {
            turns.push(TranscriptTurn {
                source,
                start_seconds: start,
                end_seconds: end,
                source_revision_start: revision,
                source_revision_end: revision,
                transcripts: vec![transcript],
            });
        }
    }
    if !include_active_turn {
        turns.pop();
    }
    turns
}

fn is_new_turn(
    turn: &TranscriptTurn<'_>,
    regenerate: bool,
    previous_revision: u64,
    previous_end: f64,
) -> bool {
    regenerate
        || turn.source_revision_end > previous_revision
        || turn.end_seconds > previous_end + f64::EPSILON
}

fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    if text.chars().count() <= max_chars {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_chars = 0;
    for character in text.chars() {
        if current_chars == max_chars {
            chunks.push(std::mem::take(&mut current));
            current_chars = 0;
        }
        current.push(character);
        current_chars += 1;
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn prompt_hash(prompt: &str) -> String {
    let digest = Sha256::digest(prompt.as_bytes());
    format!("sha256:{digest:x}")
}

fn document_from_turns(turns: Vec<IntelligentTranscriptTurn>) -> IntelligentTranscriptDocument {
    IntelligentTranscriptDocument {
        version: 2,
        covered_until: turns
            .iter()
            .map(|turn| turn.end_seconds)
            .fold(0.0, f64::max),
        source_revision: turns
            .iter()
            .map(|turn| turn.source_revision_end)
            .max()
            .unwrap_or(0),
        updated_at: turns
            .last()
            .map(|turn| turn.created_at.clone())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        turns,
    }
}

fn read_document(folder: &Path) -> Result<Option<IntelligentTranscriptDocument>, String> {
    let path = folder.join(DOCUMENT_FILE);
    if path.exists() {
        let lines = BufReader::new(
            File::open(&path)
                .map_err(|error| format!("Failed to read refined transcript: {error}"))?,
        )
        .lines()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Failed to read refined transcript: {error}"))?;
        let last_content_line = lines.iter().rposition(|line| !line.trim().is_empty());
        let mut turns = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<IntelligentTranscriptTurn>(line) {
                Ok(turn) => turns.push(turn),
                Err(_) if Some(index) == last_content_line => break,
                Err(error) => {
                    return Err(format!(
                        "Failed to parse refined transcript line {}: {error}",
                        index + 1
                    ));
                }
            }
        }
        return if turns.is_empty() {
            Ok(None)
        } else {
            Ok(Some(document_from_turns(turns)))
        };
    }

    let legacy_path = folder.join(LEGACY_DOCUMENT_FILE);
    if !legacy_path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&legacy_path)
        .map_err(|error| format!("Failed to read legacy intelligent transcript: {error}"))?;
    let legacy: LegacyIntelligentTranscriptDocument = serde_json::from_str(&content)
        .map_err(|error| format!("Failed to parse legacy intelligent transcript: {error}"))?;
    Ok(Some(document_from_turns(vec![IntelligentTranscriptTurn {
        schema_version: 1,
        turn_id: "legacy-document".to_string(),
        source: "speaker".to_string(),
        start_seconds: 0.0,
        end_seconds: legacy.covered_until,
        source_revision_start: 1,
        source_revision_end: legacy.source_revision,
        raw_text: legacy.markdown.clone(),
        content: legacy.markdown,
        created_at: legacy.updated_at,
        model: IntelligentTranscriptModelInfo {
            provider: "legacy".to_string(),
            model: "legacy".to_string(),
        },
        prompt_hash: "legacy".to_string(),
    }])))
}

fn append_turns(folder: &Path, turns: &[IntelligentTranscriptTurn]) -> Result<(), String> {
    if turns.is_empty() {
        return Ok(());
    }
    let path = folder.join(DOCUMENT_FILE);
    let needs_separator = prepare_jsonl_append(&path)?;
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("Failed to open refined transcript: {error}"))?;
    let mut writer = BufWriter::new(file);
    if needs_separator {
        writer
            .write_all(b"\n")
            .map_err(|error| format!("Failed to repair refined transcript: {error}"))?;
    }
    write_turns(&mut writer, turns)?;
    writer
        .flush()
        .map_err(|error| format!("Failed to flush refined transcript: {error}"))?;
    writer
        .get_ref()
        .sync_all()
        .map_err(|error| format!("Failed to sync refined transcript: {error}"))
}

fn prepare_jsonl_append(path: &Path) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }
    let bytes = std::fs::read(path)
        .map_err(|error| format!("Failed to inspect refined transcript: {error}"))?;
    if bytes.is_empty() || bytes.ends_with(b"\n") {
        return Ok(false);
    }
    let line_start = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let tail = std::str::from_utf8(&bytes[line_start..]).unwrap_or_default();
    if !tail.trim().is_empty()
        && serde_json::from_str::<IntelligentTranscriptTurn>(tail.trim()).is_err()
    {
        OpenOptions::new()
            .write(true)
            .open(path)
            .and_then(|file| file.set_len(line_start as u64))
            .map_err(|error| {
                format!("Failed to remove truncated refined transcript tail: {error}")
            })?;
        return Ok(false);
    }
    Ok(true)
}

fn rewrite_turns(folder: &Path, turns: &[IntelligentTranscriptTurn]) -> Result<(), String> {
    let path = folder.join(DOCUMENT_FILE);
    let temp_path = folder.join(format!(".{DOCUMENT_FILE}.tmp"));
    {
        let file = File::create(&temp_path)
            .map_err(|error| format!("Failed to create refined transcript: {error}"))?;
        let mut writer = BufWriter::new(file);
        write_turns(&mut writer, turns)?;
        writer
            .flush()
            .map_err(|error| format!("Failed to flush refined transcript: {error}"))?;
        writer
            .get_ref()
            .sync_all()
            .map_err(|error| format!("Failed to sync refined transcript: {error}"))?;
    }
    match std::fs::rename(&temp_path, &path) {
        Ok(()) => Ok(()),
        Err(first_error) if path.exists() => {
            std::fs::remove_file(&path).map_err(|error| {
                format!("Failed to replace refined transcript after {first_error}: {error}")
            })?;
            std::fs::rename(&temp_path, &path)
                .map_err(|error| format!("Failed to finalize refined transcript: {error}"))
        }
        Err(error) => Err(format!("Failed to finalize refined transcript: {error}")),
    }
}

fn write_turns(writer: &mut impl Write, turns: &[IntelligentTranscriptTurn]) -> Result<(), String> {
    for turn in turns {
        serde_json::to_writer(&mut *writer, turn)
            .map_err(|error| format!("Failed to serialize refined transcript turn: {error}"))?;
        writer
            .write_all(b"\n")
            .map_err(|error| format!("Failed to write refined transcript turn: {error}"))?;
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        sequence: u64,
        source: &str,
        text: &str,
        start: f64,
        end: f64,
    ) -> IntelligenceTranscriptInput {
        IntelligenceTranscriptInput {
            sequence_id: Some(sequence),
            source: Some(source.to_string()),
            text: text.to_string(),
            audio_start_time: Some(start),
            audio_end_time: Some(end),
        }
    }

    fn stored_turn(revision: u64, source: &str) -> IntelligentTranscriptTurn {
        IntelligentTranscriptTurn {
            schema_version: 1,
            turn_id: format!("turn-{revision}"),
            source: source.to_string(),
            start_seconds: revision as f64,
            end_seconds: revision as f64 + 1.0,
            source_revision_start: revision,
            source_revision_end: revision,
            raw_text: "raw".to_string(),
            content: "refined".to_string(),
            created_at: "2026-07-30T00:00:00Z".to_string(),
            model: IntelligentTranscriptModelInfo {
                provider: "test".to_string(),
                model: "test".to_string(),
            },
            prompt_hash: "sha256:test".to_string(),
        }
    }

    #[test]
    fn groups_consecutive_segments_and_leaves_active_turn_open() {
        let entries = vec![
            entry(1, "system", "question one", 0.0, 1.0),
            entry(2, "speaker", "question two", 1.0, 2.0),
            entry(3, "mic", "answer one", 2.0, 3.0),
            entry(4, "microphone", "answer two", 3.0, 4.0),
            entry(5, "systemAudio", "follow up", 4.0, 5.0),
        ];
        let completed = group_transcripts(&entries, false);
        assert_eq!(completed.len(), 2);
        assert_eq!(completed[0].source, "speaker");
        assert_eq!(completed[0].source_revision_start, 1);
        assert_eq!(completed[0].source_revision_end, 2);
        assert_eq!(completed[1].source, "mic");
        assert_eq!(completed[1].source_revision_start, 3);
        assert_eq!(completed[1].source_revision_end, 4);

        let finalized = group_transcripts(&entries, true);
        assert_eq!(finalized.len(), 3);
        assert_eq!(finalized[2].source, "speaker");
    }

    #[test]
    fn jsonl_append_and_atomic_rewrite_round_trip() {
        let folder = tempfile::tempdir().unwrap();
        rewrite_turns(folder.path(), &[stored_turn(1, "speaker")]).unwrap();
        append_turns(folder.path(), &[stored_turn(2, "mic")]).unwrap();
        let restored = read_document(folder.path()).unwrap().unwrap();
        assert_eq!(restored.version, 2);
        assert_eq!(restored.turns.len(), 2);
        assert_eq!(restored.source_revision, 2);
        assert!(!folder.path().join(format!(".{DOCUMENT_FILE}.tmp")).exists());
    }

    #[test]
    fn meeting_end_uses_timing_when_database_revisions_are_renumbered() {
        let entries = vec![
            entry(1, "system", "already refined", 0.0, 5.0),
            entry(2, "mic", "active final turn", 5.0, 10.0),
        ];
        let turns = group_transcripts(&entries, true);
        assert!(!is_new_turn(&turns[0], false, 10, 5.0));
        assert!(is_new_turn(&turns[1], false, 10, 5.0));
    }

    #[test]
    fn legacy_snapshot_is_available_without_modifying_it() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(
            folder.path().join(LEGACY_DOCUMENT_FILE),
            r#"{"version":1,"markdown":"legacy detail","coveredUntil":12.0,"sourceRevision":3,"updatedAt":"2026-07-30T00:00:00Z"}"#,
        )
        .unwrap();
        let restored = read_document(folder.path()).unwrap().unwrap();
        assert_eq!(restored.turns.len(), 1);
        assert_eq!(restored.turns[0].content, "legacy detail");
        assert!(!folder.path().join(DOCUMENT_FILE).exists());
    }

    #[test]
    fn malformed_final_jsonl_line_is_recovered_before_append() {
        let folder = tempfile::tempdir().unwrap();
        rewrite_turns(folder.path(), &[stored_turn(1, "speaker")]).unwrap();
        let path = folder.path().join(DOCUMENT_FILE);
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"partial\":").unwrap();
        assert_eq!(
            read_document(folder.path()).unwrap().unwrap().turns.len(),
            1
        );

        append_turns(folder.path(), &[stored_turn(2, "mic")]).unwrap();
        let recovered = read_document(folder.path()).unwrap().unwrap();
        assert_eq!(recovered.turns.len(), 2);
        assert_eq!(recovered.turns[1].turn_id, "turn-2");

        std::fs::write(&path, "{broken}\n{\"partial\":").unwrap();
        assert!(read_document(folder.path()).is_err());
    }

    #[test]
    fn prompt_hash_is_stable_and_prefixed() {
        assert_eq!(prompt_hash("same"), prompt_hash("same"));
        assert_ne!(prompt_hash("same"), prompt_hash("different"));
        assert!(prompt_hash("same").starts_with("sha256:"));
    }

    #[test]
    fn chunks_continuous_chinese_text_at_the_character_limit() {
        let input = "这是一个没有空格的中文长句";
        let chunks = chunk_text(input, 4);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 4));
        assert_eq!(chunks.concat(), input);
    }
}
