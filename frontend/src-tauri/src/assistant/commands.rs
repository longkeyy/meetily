use super::{service, AssistantSuggestionRequest, AssistantSuggestionResponse};
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
