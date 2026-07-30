pub mod commands;
pub mod realtime_summary;
pub mod service;
pub mod settings;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntelligenceTranscriptInput {
    pub sequence_id: Option<u64>,
    pub source: Option<String>,
    pub text: String,
    pub audio_start_time: Option<f64>,
    pub audio_end_time: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntelligentTranscriptDocument {
    pub version: u32,
    pub markdown: String,
    pub covered_until: f64,
    pub source_revision: u64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateIntelligentTranscriptRequest {
    pub request_id: String,
    pub meeting_folder: String,
    pub transcripts: Vec<IntelligenceTranscriptInput>,
    #[serde(default)]
    pub force_full: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntelligentTranscriptResponse {
    pub request_id: String,
    pub document: IntelligentTranscriptDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeSummaryDocument {
    pub version: u32,
    pub segments: Vec<RealtimeSummarySegment>,
    pub covered_until: f64,
    pub source_revision: u64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RealtimeSummaryTrigger {
    Interval,
    MeetingEnd,
    Manual,
    Regenerate,
    Legacy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeSummaryModelInfo {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeSummarySegment {
    pub schema_version: u32,
    pub segment_id: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub source_revision_start: u64,
    pub source_revision_end: u64,
    pub content_format: String,
    pub content: String,
    pub trigger: RealtimeSummaryTrigger,
    pub created_at: String,
    pub model: RealtimeSummaryModelInfo,
    pub prompt_hash: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateRealtimeSummaryRequest {
    pub request_id: String,
    pub meeting_folder: String,
    pub transcripts: Vec<IntelligenceTranscriptInput>,
    #[serde(default)]
    pub force_full: bool,
    #[serde(default)]
    pub trigger: Option<RealtimeSummaryTrigger>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeSummaryResponse {
    pub request_id: String,
    pub document: RealtimeSummaryDocument,
}
