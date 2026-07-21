use super::{
    service,
    settings::{self, AssistantSettingsUpdate, AssistantSettingsView},
    AssistantSuggestionRequest, AssistantSuggestionResponse,
};
use crate::state::AppState;
use tauri::{AppHandle, Runtime};

#[tauri::command]
pub async fn api_generate_assistant_suggestion<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    request: AssistantSuggestionRequest,
) -> Result<AssistantSuggestionResponse, String> {
    service::generate_suggestion(&app, state.db_manager.pool(), request).await
}

#[tauri::command]
pub async fn api_cancel_assistant_suggestion() -> bool {
    service::cancel_generation()
}

#[tauri::command]
pub async fn api_get_assistant_settings<R: Runtime>(
    app: AppHandle<R>,
) -> Result<AssistantSettingsView, String> {
    settings::get_assistant_settings(&app).await
}

#[tauri::command]
pub async fn api_save_assistant_settings<R: Runtime>(
    app: AppHandle<R>,
    settings_update: AssistantSettingsUpdate,
) -> Result<AssistantSettingsView, String> {
    settings::save_assistant_settings(&app, settings_update).await
}
