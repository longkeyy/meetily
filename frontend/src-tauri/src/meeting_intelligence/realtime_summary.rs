use super::service::{
    cleanup_background_generation, load_runtime_config, register_background_generation,
};
use super::{
    GenerateRealtimeSummaryRequest, IntelligenceTranscriptInput, RealtimeSummaryDocument,
    RealtimeSummaryResponse,
};
use crate::database::repositories::meeting::MeetingsRepository;
use crate::summary::llm_client::generate_summary;
use crate::summary::processor::clean_llm_markdown_output;
use reqwest::Client;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Runtime};

const DOCUMENT_FILE: &str = "realtime_summary.json";
const MAX_INPUT_SEGMENTS: usize = 2_000;
const MAX_CONTEXT_CHARS: usize = 10_000;
const MAX_OUTPUT_TOKENS: u32 = 1_600;

pub async fn generate_for_live_recording<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    request: GenerateRealtimeSummaryRequest,
    system_prompt: &str,
) -> Result<RealtimeSummaryResponse, String> {
    if request.request_id.trim().is_empty() {
        return Err("Request ID cannot be empty".to_string());
    }
    if request.transcripts.is_empty() {
        return Err("No transcript is available for realtime summary".to_string());
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
    Ok(RealtimeSummaryResponse {
        request_id: request.request_id,
        document,
    })
}

pub async fn regenerate_for_meeting<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    meeting_id: &str,
    system_prompt: &str,
) -> Result<RealtimeSummaryDocument, String> {
    let (folder, transcripts) = load_meeting_transcripts(pool, meeting_id).await?;
    generate_document(app, pool, &folder, &transcripts, true, system_prompt).await
}

pub async fn load_for_meeting(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<Option<RealtimeSummaryDocument>, String> {
    let meeting = MeetingsRepository::get_meeting_metadata(pool, meeting_id)
        .await
        .map_err(|error| format!("Failed to load meeting: {error}"))?
        .ok_or_else(|| "Meeting not found".to_string())?;
    let Some(folder) = meeting.folder_path.map(PathBuf::from) else {
        return Ok(None);
    };
    read_document(&folder)
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
    force_full: bool,
    system_prompt: &str,
) -> Result<RealtimeSummaryDocument, String> {
    let previous = if force_full {
        None
    } else {
        read_document(folder)?
    };
    let covered_until = previous
        .as_ref()
        .map(|document| document.covered_until)
        .unwrap_or(0.0);
    let source_revision = previous
        .as_ref()
        .map(|document| document.source_revision)
        .unwrap_or(0);
    let selected = if force_full {
        transcripts.iter().collect::<Vec<_>>()
    } else {
        transcripts
            .iter()
            .filter(|segment| is_new_segment(segment, source_revision, covered_until))
            .collect::<Vec<_>>()
    };
    if selected.is_empty() {
        return previous.ok_or_else(|| "No new transcript is available".to_string());
    }

    let config = load_runtime_config(app, pool).await?;
    let client = Client::new();
    let mut markdown = previous
        .as_ref()
        .map(|document| document.markdown.clone())
        .unwrap_or_default();
    let (generation_id, cancellation_token) = register_background_generation();
    let generation_result = async {
        for chunk in chunk_transcripts(&selected, MAX_CONTEXT_CHARS) {
            let context = build_transcript_context(&chunk);
            let previous_summary = if markdown.is_empty() { "(none)" } else { markdown.as_str() };
            let user_prompt = format!(
                "Previous cumulative summary:\n{previous_summary}\n\nNew raw transcript since the previous checkpoint:\n{context}\n\nReturn the complete cumulative realtime summary only."
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
                config.temperature.or(Some(0.15)),
                config.top_p.or(Some(0.9)),
                config.app_data_dir.as_ref(),
                Some(&cancellation_token),
            )
            .await?;
            markdown = clean_llm_markdown_output(&output);
            if markdown.is_empty() {
                return Err("The model returned an empty realtime summary".to_string());
            }
        }
        Ok::<(), String>(())
    }
    .await;
    cleanup_background_generation(generation_id);
    generation_result?;

    let document = RealtimeSummaryDocument {
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

fn source_label(source: Option<&str>) -> &'static str {
    match source {
        Some("mic") | Some("microphone") => "mic",
        _ => "speaker",
    }
}

fn is_new_segment(
    segment: &IntelligenceTranscriptInput,
    source_revision: u64,
    covered_until: f64,
) -> bool {
    segment
        .sequence_id
        .is_some_and(|revision| revision > source_revision)
        || segment.audio_end_time.unwrap_or(0.0) > covered_until
}

fn build_transcript_context(transcripts: &[&IntelligenceTranscriptInput]) -> String {
    transcripts
        .iter()
        .filter_map(|segment| {
            let text = segment.text.trim();
            if text.is_empty() {
                return None;
            }
            let seconds = segment.audio_start_time.unwrap_or(0.0).max(0.0) as u64;
            Some(format!(
                "[{:02}:{:02}] {}: {}",
                seconds / 60,
                seconds % 60,
                source_label(segment.source.as_deref()),
                text
            ))
        })
        .collect::<Vec<_>>()
        .join("\n")
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

fn read_document(folder: &Path) -> Result<Option<RealtimeSummaryDocument>, String> {
    let path = folder.join(DOCUMENT_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read realtime summary: {error}"))?;
    serde_json::from_str(&content)
        .map(Some)
        .map_err(|error| format!("Failed to parse realtime summary: {error}"))
}

fn write_document(folder: &Path, document: &RealtimeSummaryDocument) -> Result<(), String> {
    let path = folder.join(DOCUMENT_FILE);
    let temp_path = folder.join(format!(".{DOCUMENT_FILE}.tmp"));
    let json = serde_json::to_string_pretty(document)
        .map_err(|error| format!("Failed to serialize realtime summary: {error}"))?;
    std::fs::write(&temp_path, json)
        .map_err(|error| format!("Failed to write realtime summary: {error}"))?;
    match std::fs::rename(&temp_path, &path) {
        Ok(()) => Ok(()),
        Err(first_error) if path.exists() => {
            std::fs::remove_file(&path).map_err(|error| {
                format!("Failed to replace realtime summary after {first_error}: {error}")
            })?;
            std::fs::rename(&temp_path, &path)
                .map_err(|error| format!("Failed to finalize realtime summary: {error}"))
        }
        Err(error) => Err(format!("Failed to finalize realtime summary: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_preserves_source_roles_and_time_order() {
        let entries = [
            IntelligenceTranscriptInput {
                sequence_id: Some(1),
                source: Some("system".to_string()),
                text: "What did you test?".to_string(),
                audio_start_time: Some(120.0),
                audio_end_time: Some(123.0),
            },
            IntelligenceTranscriptInput {
                sequence_id: Some(2),
                source: Some("mic".to_string()),
                text: "API and UI automation".to_string(),
                audio_start_time: Some(124.0),
                audio_end_time: Some(127.0),
            },
        ];
        let refs = entries.iter().collect::<Vec<_>>();
        let context = build_transcript_context(&refs);
        assert!(context.starts_with("[02:00] speaker:"));
        assert!(context.contains("[02:04] mic:"));
    }

    #[test]
    fn realtime_summary_document_round_trips_atomically() {
        let folder = tempfile::tempdir().unwrap();
        let document = RealtimeSummaryDocument {
            version: 1,
            markdown: "## 讨论主题\n自动化测试".to_string(),
            covered_until: 120.0,
            source_revision: 5,
            updated_at: "2026-07-30T00:00:00Z".to_string(),
        };
        write_document(folder.path(), &document).unwrap();
        let updated = RealtimeSummaryDocument {
            markdown: "## 讨论主题\n性能测试".to_string(),
            source_revision: 6,
            ..document.clone()
        };
        write_document(folder.path(), &updated).unwrap();
        assert_eq!(
            read_document(folder.path()).unwrap().unwrap().markdown,
            updated.markdown
        );
        assert!(!folder.path().join(format!(".{DOCUMENT_FILE}.tmp")).exists());
    }

    #[test]
    fn sequence_revision_detects_new_segments_without_end_times() {
        let segment = IntelligenceTranscriptInput {
            sequence_id: Some(8),
            source: Some("mic".to_string()),
            text: "Follow-up".to_string(),
            audio_start_time: None,
            audio_end_time: None,
        };
        assert!(is_new_segment(&segment, 7, 120.0));
        assert!(!is_new_segment(&segment, 8, 120.0));
    }
}
