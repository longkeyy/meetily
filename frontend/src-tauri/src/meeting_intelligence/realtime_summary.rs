use super::service::{
    cleanup_background_generation, load_runtime_config, register_background_generation,
};
use super::{
    GenerateRealtimeSummaryRequest, IntelligenceTranscriptInput, RealtimeSummaryDocument,
    RealtimeSummaryModelInfo, RealtimeSummaryResponse, RealtimeSummarySegment,
    RealtimeSummaryTrigger,
};
use crate::database::repositories::meeting::MeetingsRepository;
use crate::summary::llm_client::generate_summary;
use crate::summary::processor::clean_llm_markdown_output;
use reqwest::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Runtime};

const DOCUMENT_FILE: &str = "realtime_summary.jsonl";
const LEGACY_DOCUMENT_FILE: &str = "realtime_summary.json";
const MAX_INPUT_SEGMENTS: usize = 2_000;
const MAX_CONTEXT_CHARS: usize = 10_000;
const MAX_OUTPUT_TOKENS: u32 = 1_600;

static DOCUMENT_WRITE_LOCK: Mutex<()> = Mutex::new(());

struct TranscriptWindow<'a> {
    start_seconds: f64,
    end_seconds: f64,
    transcripts: Vec<&'a IntelligenceTranscriptInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyRealtimeSummaryDocument {
    markdown: String,
    covered_until: f64,
    source_revision: u64,
    updated_at: String,
}

pub async fn generate_for_live_recording<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    request: GenerateRealtimeSummaryRequest,
    system_prompt: &str,
) -> Result<RealtimeSummaryResponse, String> {
    validate_request(&request)?;
    let folder = PathBuf::from(request.meeting_folder.trim());
    if !folder.is_dir() {
        return Err("Meeting folder does not exist".to_string());
    }
    let settings = super::settings::load_settings(app);
    let trigger = request.trigger.unwrap_or(RealtimeSummaryTrigger::Interval);
    let document = generate_document(
        app,
        pool,
        &folder,
        &request.transcripts,
        request.force_full,
        trigger,
        settings.realtime_summary_interval_seconds,
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
    interval_seconds: u32,
    system_prompt: &str,
) -> Result<RealtimeSummaryDocument, String> {
    let (folder, transcripts) = load_meeting_transcripts(pool, meeting_id).await?;
    generate_document(
        app,
        pool,
        &folder,
        &transcripts,
        true,
        RealtimeSummaryTrigger::Regenerate,
        interval_seconds,
        system_prompt,
    )
    .await
}

pub async fn finalize_for_meeting<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    meeting_id: &str,
    interval_seconds: u32,
    system_prompt: &str,
) -> Result<RealtimeSummaryDocument, String> {
    let (folder, transcripts) = load_meeting_transcripts(pool, meeting_id).await?;
    generate_document(
        app,
        pool,
        &folder,
        &transcripts,
        false,
        RealtimeSummaryTrigger::MeetingEnd,
        interval_seconds,
        system_prompt,
    )
    .await
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

fn validate_request(request: &GenerateRealtimeSummaryRequest) -> Result<(), String> {
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
    force_full: bool,
    trigger: RealtimeSummaryTrigger,
    interval_seconds: u32,
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

    let windows = partition_transcripts(&selected, covered_until, interval_seconds.max(1));
    let config = load_runtime_config(app, pool).await?;
    let model = RealtimeSummaryModelInfo {
        provider: config.provider_name.clone(),
        model: config.model.clone(),
    };
    let prompt_hash = prompt_hash(system_prompt);
    let client = Client::new();
    let (generation_id, cancellation_token) = register_background_generation();
    let generation_result = async {
        let mut generated = Vec::with_capacity(windows.len());
        for window in windows {
            let content = generate_window_content(
                &client,
                &config,
                &window.transcripts,
                system_prompt,
                &cancellation_token,
            )
            .await?;
            generated.push(RealtimeSummarySegment {
                schema_version: 1,
                segment_id: uuid::Uuid::new_v4().to_string(),
                start_seconds: window.start_seconds,
                end_seconds: window.end_seconds,
                source_revision_start: window
                    .transcripts
                    .iter()
                    .filter_map(|segment| segment.sequence_id)
                    .min()
                    .unwrap_or(source_revision.saturating_add(1)),
                source_revision_end: window
                    .transcripts
                    .iter()
                    .filter_map(|segment| segment.sequence_id)
                    .max()
                    .unwrap_or(source_revision),
                content_format: "markdown".to_string(),
                content,
                trigger,
                created_at: chrono::Utc::now().to_rfc3339(),
                model: model.clone(),
                prompt_hash: prompt_hash.clone(),
            });
        }
        Ok::<Vec<RealtimeSummarySegment>, String>(generated)
    }
    .await;
    cleanup_background_generation(generation_id);
    let generated = generation_result?;

    let mut segments = previous
        .as_ref()
        .map(|document| document.segments.clone())
        .unwrap_or_default();
    segments.extend(generated.iter().cloned());
    if force_full {
        rewrite_segments(folder, &segments)?;
    } else if folder.join(DOCUMENT_FILE).exists() {
        append_segments(folder, &generated)?;
    } else {
        rewrite_segments(folder, &segments)?;
    }
    Ok(document_from_segments(segments))
}

async fn generate_window_content(
    client: &Client,
    config: &super::service::LlmRuntimeConfig,
    transcripts: &[&IntelligenceTranscriptInput],
    system_prompt: &str,
    cancellation_token: &tokio_util::sync::CancellationToken,
) -> Result<String, String> {
    let mut markdown = String::new();
    for chunk in chunk_transcripts(transcripts, MAX_CONTEXT_CHARS) {
        let context = build_transcript_context(&chunk);
        let draft = if markdown.is_empty() {
            "(none)"
        } else {
            markdown.as_str()
        };
        let user_prompt = format!(
            "Current interval draft (only use this when the interval was split for input size):\n{draft}\n\nRaw transcript for this interval:\n{context}\n\nReturn only the complete summary for this interval. Do not include prior intervals."
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
            Some(cancellation_token),
        )
        .await?;
        markdown = clean_llm_markdown_output(&output);
        if markdown.is_empty() {
            return Err("The model returned an empty realtime summary".to_string());
        }
    }
    Ok(markdown)
}

fn partition_transcripts<'a>(
    transcripts: &[&'a IntelligenceTranscriptInput],
    initial_start: f64,
    interval_seconds: u32,
) -> Vec<TranscriptWindow<'a>> {
    let interval = interval_seconds as f64;
    let mut windows = Vec::new();
    let mut current = Vec::new();
    let mut window_start = initial_start.max(0.0);
    let mut window_end = window_start + interval;

    for transcript in transcripts {
        let transcript_end = transcript
            .audio_end_time
            .or(transcript.audio_start_time)
            .unwrap_or(window_start)
            .max(window_start);
        if !current.is_empty() && transcript_end > window_end {
            windows.push(TranscriptWindow {
                start_seconds: window_start,
                end_seconds: window_end,
                transcripts: std::mem::take(&mut current),
            });
            window_start = window_end;
            window_end += interval;
        }
        while current.is_empty() && transcript_end > window_end {
            window_start = window_end;
            window_end += interval;
        }
        current.push(*transcript);
    }
    if !current.is_empty() {
        let actual_end = current
            .iter()
            .filter_map(|segment| segment.audio_end_time.or(segment.audio_start_time))
            .fold(window_start, f64::max);
        windows.push(TranscriptWindow {
            start_seconds: window_start,
            end_seconds: actual_end.max(window_start),
            transcripts: current,
        });
    }
    windows
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

fn prompt_hash(prompt: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(prompt.as_bytes()))
}

fn document_from_segments(segments: Vec<RealtimeSummarySegment>) -> RealtimeSummaryDocument {
    RealtimeSummaryDocument {
        version: 1,
        covered_until: segments
            .iter()
            .map(|segment| segment.end_seconds)
            .fold(0.0, f64::max),
        source_revision: segments
            .iter()
            .map(|segment| segment.source_revision_end)
            .max()
            .unwrap_or(0),
        updated_at: segments
            .last()
            .map(|segment| segment.created_at.clone())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        segments,
    }
}

fn read_document(folder: &Path) -> Result<Option<RealtimeSummaryDocument>, String> {
    let path = folder.join(DOCUMENT_FILE);
    if path.exists() {
        let file = File::open(&path)
            .map_err(|error| format!("Failed to read realtime summary: {error}"))?;
        let lines = BufReader::new(file)
            .lines()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Failed to read realtime summary: {error}"))?;
        let last_content_line = lines.iter().rposition(|line| !line.trim().is_empty());
        let mut segments = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<RealtimeSummarySegment>(line) {
                Ok(segment) => segments.push(segment),
                Err(_) if Some(index) == last_content_line => break,
                Err(error) => {
                    return Err(format!(
                        "Failed to parse realtime summary line {}: {error}",
                        index + 1
                    ));
                }
            }
        }
        return if segments.is_empty() {
            Ok(None)
        } else {
            Ok(Some(document_from_segments(segments)))
        };
    }

    let legacy_path = folder.join(LEGACY_DOCUMENT_FILE);
    if !legacy_path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&legacy_path)
        .map_err(|error| format!("Failed to read legacy realtime summary: {error}"))?;
    let legacy: LegacyRealtimeSummaryDocument = serde_json::from_str(&content)
        .map_err(|error| format!("Failed to parse legacy realtime summary: {error}"))?;
    let segment = RealtimeSummarySegment {
        schema_version: 1,
        segment_id: "legacy-import".to_string(),
        start_seconds: 0.0,
        end_seconds: legacy.covered_until,
        source_revision_start: 0,
        source_revision_end: legacy.source_revision,
        content_format: "markdown".to_string(),
        content: legacy.markdown,
        trigger: RealtimeSummaryTrigger::Legacy,
        created_at: legacy.updated_at,
        model: RealtimeSummaryModelInfo {
            provider: "legacy".to_string(),
            model: "legacy".to_string(),
        },
        prompt_hash: "sha256:legacy".to_string(),
    };
    Ok(Some(document_from_segments(vec![segment])))
}

fn append_segments(folder: &Path, segments: &[RealtimeSummarySegment]) -> Result<(), String> {
    if segments.is_empty() {
        return Ok(());
    }
    let _guard = DOCUMENT_WRITE_LOCK
        .lock()
        .map_err(|_| "Realtime summary write lock is poisoned".to_string())?;
    let path = folder.join(DOCUMENT_FILE);
    let needs_separator = prepare_jsonl_append(&path)?;
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("Failed to open realtime summary: {error}"))?;
    let mut writer = BufWriter::new(file);
    if needs_separator {
        writer
            .write_all(b"\n")
            .map_err(|error| format!("Failed to repair realtime summary: {error}"))?;
    }
    write_jsonl(&mut writer, segments)?;
    writer
        .flush()
        .map_err(|error| format!("Failed to flush realtime summary: {error}"))?;
    writer
        .get_ref()
        .sync_all()
        .map_err(|error| format!("Failed to sync realtime summary: {error}"))
}

fn prepare_jsonl_append(path: &Path) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }
    let bytes = std::fs::read(path)
        .map_err(|error| format!("Failed to inspect realtime summary: {error}"))?;
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
        && serde_json::from_str::<RealtimeSummarySegment>(tail.trim()).is_err()
    {
        OpenOptions::new()
            .write(true)
            .open(path)
            .and_then(|file| file.set_len(line_start as u64))
            .map_err(|error| {
                format!("Failed to remove truncated realtime summary tail: {error}")
            })?;
        return Ok(false);
    }
    Ok(true)
}

fn rewrite_segments(folder: &Path, segments: &[RealtimeSummarySegment]) -> Result<(), String> {
    let _guard = DOCUMENT_WRITE_LOCK
        .lock()
        .map_err(|_| "Realtime summary write lock is poisoned".to_string())?;
    let path = folder.join(DOCUMENT_FILE);
    let temp_path = folder.join(format!(".{DOCUMENT_FILE}.tmp"));
    let file = File::create(&temp_path)
        .map_err(|error| format!("Failed to create realtime summary: {error}"))?;
    let mut writer = BufWriter::new(file);
    write_jsonl(&mut writer, segments)?;
    writer
        .flush()
        .map_err(|error| format!("Failed to flush realtime summary: {error}"))?;
    writer
        .get_ref()
        .sync_all()
        .map_err(|error| format!("Failed to sync realtime summary: {error}"))?;
    drop(writer);
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

fn write_jsonl(
    writer: &mut BufWriter<File>,
    segments: &[RealtimeSummarySegment],
) -> Result<(), String> {
    for segment in segments {
        serde_json::to_writer(&mut *writer, segment)
            .map_err(|error| format!("Failed to serialize realtime summary: {error}"))?;
        writer
            .write_all(b"\n")
            .map_err(|error| format!("Failed to write realtime summary: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(id: &str, start: f64, end: f64) -> RealtimeSummarySegment {
        RealtimeSummarySegment {
            schema_version: 1,
            segment_id: id.to_string(),
            start_seconds: start,
            end_seconds: end,
            source_revision_start: 1,
            source_revision_end: 2,
            content_format: "markdown".to_string(),
            content: format!("## 讨论主题\n{id}"),
            trigger: RealtimeSummaryTrigger::Interval,
            created_at: "2026-07-30T00:00:00Z".to_string(),
            model: RealtimeSummaryModelInfo {
                provider: "ollama".to_string(),
                model: "test".to_string(),
            },
            prompt_hash: "sha256:test".to_string(),
        }
    }

    #[test]
    fn jsonl_round_trip_preserves_order_and_append() {
        let folder = tempfile::tempdir().unwrap();
        rewrite_segments(folder.path(), &[segment("first", 0.0, 120.0)]).unwrap();
        append_segments(folder.path(), &[segment("second", 120.0, 180.0)]).unwrap();
        let document = read_document(folder.path()).unwrap().unwrap();
        assert_eq!(document.segments.len(), 2);
        assert_eq!(document.segments[1].content, "## 讨论主题\nsecond");
        assert_eq!(document.covered_until, 180.0);
    }

    #[test]
    fn malformed_final_line_is_ignored_but_earlier_corruption_fails() {
        let folder = tempfile::tempdir().unwrap();
        rewrite_segments(folder.path(), &[segment("valid", 0.0, 120.0)]).unwrap();
        let path = folder.path().join(DOCUMENT_FILE);
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"partial\":").unwrap();
        assert_eq!(
            read_document(folder.path())
                .unwrap()
                .unwrap()
                .segments
                .len(),
            1
        );
        append_segments(folder.path(), &[segment("recovered", 120.0, 180.0)]).unwrap();
        let recovered = read_document(folder.path()).unwrap().unwrap();
        assert_eq!(recovered.segments.len(), 2);
        assert_eq!(recovered.segments[1].segment_id, "recovered");

        std::fs::write(&path, "{broken}\n{\"partial\":").unwrap();
        assert!(read_document(folder.path()).is_err());
    }

    #[test]
    fn legacy_json_is_loaded_and_migrated_on_rewrite() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(
            folder.path().join(LEGACY_DOCUMENT_FILE),
            r#"{"version":1,"markdown":"legacy content","coveredUntil":90.0,"sourceRevision":4,"updatedAt":"2026-07-30T00:00:00Z"}"#,
        )
        .unwrap();
        let legacy = read_document(folder.path()).unwrap().unwrap();
        assert_eq!(legacy.segments[0].trigger, RealtimeSummaryTrigger::Legacy);
        rewrite_segments(folder.path(), &legacy.segments).unwrap();
        assert!(folder.path().join(LEGACY_DOCUMENT_FILE).exists());
        assert!(folder.path().join(DOCUMENT_FILE).exists());
    }

    #[test]
    fn interval_partitioning_keeps_time_order() {
        let entries = [
            IntelligenceTranscriptInput {
                sequence_id: Some(1),
                source: Some("system".to_string()),
                text: "first".to_string(),
                audio_start_time: Some(5.0),
                audio_end_time: Some(10.0),
            },
            IntelligenceTranscriptInput {
                sequence_id: Some(2),
                source: Some("mic".to_string()),
                text: "second".to_string(),
                audio_start_time: Some(125.0),
                audio_end_time: Some(130.0),
            },
        ];
        let refs = entries.iter().collect::<Vec<_>>();
        let windows = partition_transcripts(&refs, 0.0, 120);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].start_seconds, 0.0);
        assert_eq!(windows[0].end_seconds, 120.0);
        assert_eq!(windows[1].start_seconds, 120.0);
        assert_eq!(windows[1].end_seconds, 130.0);
    }

    #[test]
    fn context_preserves_source_roles() {
        let entry = IntelligenceTranscriptInput {
            sequence_id: Some(1),
            source: Some("mic".to_string()),
            text: "API and UI automation".to_string(),
            audio_start_time: Some(124.0),
            audio_end_time: Some(127.0),
        };
        assert!(build_transcript_context(&[&entry]).contains("[02:04] mic:"));
    }
}
