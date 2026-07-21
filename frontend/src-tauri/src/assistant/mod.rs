pub mod commands;
mod profiles;
mod service;
pub mod settings;

use crate::audio::recording_state::TranscriptSource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssistantProfile {
    Interview,
}

impl Default for AssistantProfile {
    fn default() -> Self {
        Self::Interview
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SuggestionTrigger {
    Periodic,
    TurnEnd,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantTranscript {
    pub source: TranscriptSource,
    pub text: String,
    pub audio_start_time: f64,
    pub audio_end_time: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSuggestionRequest {
    pub request_id: String,
    pub profile: AssistantProfile,
    pub trigger: SuggestionTrigger,
    pub focus_start_time: Option<f64>,
    pub transcripts: Vec<AssistantTranscript>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSuggestionResponse {
    pub request_id: String,
    pub profile: AssistantProfile,
    pub trigger: SuggestionTrigger,
    pub suggestion: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_deserializes_frontend_camel_case_contract() {
        let request: AssistantSuggestionRequest = serde_json::from_value(serde_json::json!({
            "requestId": "request-1",
            "profile": "interview",
            "trigger": "turnEnd",
            "focusStartTime": 30.0,
            "transcripts": [{
                "source": "system",
                "text": "How would you measure success?",
                "audioStartTime": 30.0,
                "audioEndTime": 34.0
            }]
        }))
        .unwrap();

        assert_eq!(request.request_id, "request-1");
        assert_eq!(request.profile, AssistantProfile::Interview);
        assert_eq!(request.trigger, SuggestionTrigger::TurnEnd);
        assert_eq!(request.transcripts[0].source, TranscriptSource::SystemAudio);
    }
}
