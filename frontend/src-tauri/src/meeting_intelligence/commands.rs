use super::{
    service, settings, GenerateIntelligentTranscriptRequest, IntelligentTranscriptDocument,
    IntelligentTranscriptResponse,
};
use crate::state::AppState;
use tauri::{AppHandle, Runtime};

#[tauri::command]
pub async fn api_get_meeting_intelligence_settings<R: Runtime>(
    app: AppHandle<R>,
) -> Result<settings::MeetingIntelligenceSettingsView, String> {
    Ok(settings::load_settings(&app).into())
}

#[tauri::command]
pub async fn api_save_meeting_intelligence_settings<R: Runtime>(
    app: AppHandle<R>,
    settings_update: settings::MeetingIntelligenceSettingsUpdate,
) -> Result<settings::MeetingIntelligenceSettingsView, String> {
    settings::save_settings(&app, settings_update).map(Into::into)
}

#[tauri::command]
pub async fn api_generate_intelligent_transcript<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    request: GenerateIntelligentTranscriptRequest,
) -> Result<IntelligentTranscriptResponse, String> {
    let intelligence_settings = settings::load_settings(&app);
    if !intelligence_settings.intelligent_transcript_enabled {
        return Err("Intelligent recording is disabled".to_string());
    }
    service::generate_for_live_recording(
        &app,
        state.db_manager.pool(),
        request,
        &intelligence_settings.intelligent_transcript_prompt,
    )
    .await
}

#[tauri::command]
pub async fn api_get_intelligent_transcript(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Option<IntelligentTranscriptDocument>, String> {
    service::load_for_meeting(state.db_manager.pool(), &meeting_id).await
}

#[tauri::command]
pub async fn api_regenerate_intelligent_transcript<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<IntelligentTranscriptDocument, String> {
    let intelligence_settings = settings::load_settings(&app);
    service::regenerate_for_meeting(
        &app,
        state.db_manager.pool(),
        &meeting_id,
        &intelligence_settings.intelligent_transcript_prompt,
    )
    .await
}
