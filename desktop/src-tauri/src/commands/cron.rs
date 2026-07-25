use crate::AppState;
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

#[derive(Serialize)]
pub struct CronJobInfo {
    pub id: String,
    pub name: String,
    pub cron_expr: String,
    pub timezone: String,
    pub prompt: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking: String,
    pub auto_approve: bool,
    pub deliver_to: Option<String>,
    pub enabled: bool,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub created_at: String,
    pub profile_name: Option<String>,
}

#[derive(Serialize)]
pub struct CronJobRunInfo {
    pub id: String,
    pub job_id: String,
    pub job_name: String,
    pub status: String,
    pub content: Option<String>,
    pub error: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost: f64,
    pub started_at: String,
    pub completed_at: Option<String>,
}

fn job_to_info(j: &opencrabs::db::models::CronJob) -> CronJobInfo {
    CronJobInfo {
        id: j.id.to_string(),
        name: j.name.clone(),
        cron_expr: j.cron_expr.clone(),
        timezone: j.timezone.clone(),
        prompt: j.prompt.clone(),
        provider: j.provider.clone(),
        model: j.model.clone(),
        thinking: j.thinking.clone(),
        auto_approve: j.auto_approve,
        deliver_to: j.deliver_to.clone(),
        enabled: j.enabled,
        last_run_at: j.last_run_at.map(|d| d.to_rfc3339()),
        next_run_at: j.next_run_at.map(|d| d.to_rfc3339()),
        created_at: j.created_at.to_rfc3339(),
        profile_name: j.profile_name.clone(),
    }
}

fn run_to_info(r: &opencrabs::db::models::CronJobRun) -> CronJobRunInfo {
    CronJobRunInfo {
        id: r.id.to_string(),
        job_id: r.job_id.to_string(),
        job_name: r.job_name.clone(),
        status: r.status.clone(),
        content: r.content.clone(),
        error: r.error.clone(),
        input_tokens: r.input_tokens,
        output_tokens: r.output_tokens,
        cost: r.cost,
        started_at: r.started_at.to_rfc3339(),
        completed_at: r.completed_at.map(|d| d.to_rfc3339()),
    }
}

#[tauri::command]
pub async fn list_cron_jobs(state: State<'_, AppState>) -> Result<Vec<CronJobInfo>, String> {
    let sm = state.service_manager.lock().await;
    let sm = sm.as_ref().ok_or("Service not initialized")?;
    let repo = opencrabs::db::repository::CronJobRepository::new(sm.context().pool());
    let jobs = repo.list_all().await.map_err(|e| e.to_string())?;
    Ok(jobs.iter().map(job_to_info).collect())
}

#[tauri::command]
pub async fn create_cron_job(
    state: State<'_, AppState>,
    name: String,
    cron_expr: String,
    timezone: String,
    prompt: String,
    provider: Option<String>,
    model: Option<String>,
    auto_approve: bool,
    deliver_to: Option<String>,
) -> Result<CronJobInfo, String> {
    let sm = state.service_manager.lock().await;
    let sm = sm.as_ref().ok_or("Service not initialized")?;
    let repo = opencrabs::db::repository::CronJobRepository::new(sm.context().pool());
    let now = chrono::Utc::now();
    let job = opencrabs::db::models::CronJob {
        id: Uuid::new_v4(),
        name,
        cron_expr,
        timezone,
        prompt,
        provider,
        model,
        thinking: "disabled".to_string(),
        auto_approve,
        deliver_to,
        deliver_api_key: None,
        enabled: true,
        last_run_at: None,
        next_run_at: None,
        created_at: now,
        updated_at: now,
        profile_name: None,
    };
    repo.insert(&job).await.map_err(|e| e.to_string())?;
    Ok(job_to_info(&job))
}

#[tauri::command]
pub async fn delete_cron_job(state: State<'_, AppState>, job_id: String) -> Result<(), String> {
    let sm = state.service_manager.lock().await;
    let sm = sm.as_ref().ok_or("Service not initialized")?;
    let repo = opencrabs::db::repository::CronJobRepository::new(sm.context().pool());
    repo.delete(&job_id).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn toggle_cron_job(
    state: State<'_, AppState>,
    job_id: String,
    enabled: bool,
) -> Result<(), String> {
    let sm = state.service_manager.lock().await;
    let sm = sm.as_ref().ok_or("Service not initialized")?;
    let repo = opencrabs::db::repository::CronJobRepository::new(sm.context().pool());
    repo.set_enabled(&job_id, enabled)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn trigger_cron_job(state: State<'_, AppState>, job_id: String) -> Result<(), String> {
    let sm = state.service_manager.lock().await;
    let sm = sm.as_ref().ok_or("Service not initialized")?;
    let repo = opencrabs::db::repository::CronJobRepository::new(sm.context().pool());
    repo.trigger_now(&job_id).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn list_cron_runs(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<Vec<CronJobRunInfo>, String> {
    let sm = state.service_manager.lock().await;
    let sm = sm.as_ref().ok_or("Service not initialized")?;
    let repo = opencrabs::db::repository::CronJobRunRepository::new(sm.context().pool());
    let runs = repo
        .list_by_job(&job_id, 50)
        .await
        .map_err(|e| e.to_string())?;
    Ok(runs.iter().map(run_to_info).collect())
}
