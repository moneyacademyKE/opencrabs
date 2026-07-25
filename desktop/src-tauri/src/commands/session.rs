use crate::AppState;
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

#[derive(Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub title: String,
    pub model: Option<String>,
    pub provider_name: Option<String>,
    pub working_directory: Option<String>,
    pub token_count: i64,
    pub total_cost: f64,
    pub created_at: String,
    pub updated_at: String,
    pub is_archived: bool,
}

#[derive(Serialize)]
pub struct MessageInfo {
    pub id: String,
    pub role: String,
    pub content: String,
    pub sequence: i32,
    pub token_count: Option<i64>,
    pub cost: Option<f64>,
    pub created_at: String,
}

fn session_to_info(s: &opencrabs::db::models::Session) -> SessionInfo {
    SessionInfo {
        id: s.id.to_string(),
        title: s.title.clone().unwrap_or_else(|| "Untitled".into()),
        model: s.model.clone(),
        provider_name: s.provider_name.clone(),
        working_directory: s.working_directory.clone(),
        token_count: s.token_count,
        total_cost: s.total_cost,
        created_at: s.created_at.to_rfc3339(),
        updated_at: s.updated_at.to_rfc3339(),
        is_archived: s.is_archived(),
    }
}

fn message_to_info(m: &opencrabs::db::models::Message) -> MessageInfo {
    MessageInfo {
        id: m.id.to_string(),
        role: m.role.clone(),
        content: m.content.clone(),
        sequence: m.sequence,
        token_count: m.token_count,
        cost: m.cost,
        created_at: m.created_at.to_rfc3339(),
    }
}

#[tauri::command]
pub async fn list_sessions(state: State<'_, AppState>) -> Result<Vec<SessionInfo>, String> {
    let sm = state.service_manager.lock().await;
    let sm = sm.as_ref().ok_or("Service not initialized")?;

    let sessions = sm
        .sessions()
        .list_sessions(opencrabs::db::repository::SessionListOptions {
            include_archived: false,
            limit: Some(100),
            offset: 0,
            query: None,
        })
        .await
        .map_err(|e| e.to_string())?;

    Ok(sessions.iter().map(session_to_info).collect())
}

#[tauri::command]
pub async fn create_session(
    state: State<'_, AppState>,
    title: String,
) -> Result<SessionInfo, String> {
    let sm = state.service_manager.lock().await;
    let sm = sm.as_ref().ok_or("Service not initialized")?;
    let session = sm
        .sessions()
        .create_session(Some(title))
        .await
        .map_err(|e| e.to_string())?;
    Ok(session_to_info(&session))
}

#[tauri::command]
pub async fn rename_session(
    state: State<'_, AppState>,
    session_id: String,
    title: String,
) -> Result<(), String> {
    let sm = state.service_manager.lock().await;
    let sm = sm.as_ref().ok_or("Service not initialized")?;
    let id = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    sm.sessions()
        .update_session_title(id, Some(title))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_session(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    let sm = state.service_manager.lock().await;
    let sm = sm.as_ref().ok_or("Service not initialized")?;
    let id = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    sm.sessions()
        .delete_session(id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_session_messages(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<MessageInfo>, String> {
    let sm = state.service_manager.lock().await;
    let sm = sm.as_ref().ok_or("Service not initialized")?;
    let id = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    let messages = sm
        .messages()
        .list_messages_for_session(id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(messages.iter().map(message_to_info).collect())
}
