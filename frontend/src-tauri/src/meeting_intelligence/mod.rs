pub mod commands;
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
