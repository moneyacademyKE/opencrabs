//! JSON-RPC method routing: one arm per A2A method, unknown methods answer
//! `METHOD_NOT_FOUND`.

use super::stores::{CancelStore, TaskStore};
use super::{agent_cmds, notify, send, sessions, tasks};
use crate::a2a::types::*;
use crate::brain::agent::service::AgentService;
use crate::services::ServiceContext;
use std::sync::Arc;

/// Dispatch a JSON-RPC request to the appropriate handler.
pub async fn dispatch(
    req: JsonRpcRequest,
    store: TaskStore,
    cancel_store: CancelStore,
    agent_service: Arc<AgentService>,
    service_context: ServiceContext,
) -> JsonRpcResponse {
    match req.method.as_str() {
        "message/send" => {
            send::handle_send_message(
                req.id,
                req.params,
                store,
                cancel_store,
                agent_service,
                service_context,
            )
            .await
        }
        "session/notify" => {
            // #1802: the kill switch rides the config mirror, not the
            // request, so a caller cannot talk a disabled gateway into it.
            let enabled = crate::config::Config::current()
                .agent
                .session_notify_enabled;
            notify::handle_session_notify(req.id, req.params, service_context, enabled).await
        }
        "session/notify-status" => notify::handle_notify_status(req.id, req.params),
        "tasks/get" => tasks::handle_get_task(req.id, req.params, store).await,
        "tasks/cancel" => {
            tasks::handle_cancel_task(
                req.id,
                req.params,
                store,
                cancel_store,
                &service_context.pool(),
            )
            .await
        }
        "sessions/list" => {
            sessions::handle_list(req.id, req.params, service_context.clone()).await
        }
        "sessions/create" => {
            sessions::handle_create(req.id, req.params, service_context.clone()).await
        }
        "sessions/history" => {
            sessions::handle_history(req.id, req.params, service_context.clone()).await
        }
        "sessions/rename" => {
            sessions::handle_rename(req.id, req.params, service_context.clone()).await
        }
        "sessions/delete" => {
            sessions::handle_delete(req.id, req.params, service_context.clone()).await
        }
        "agent/commands" => agent_cmds::handle_commands(req.id, req.params).await,
        "agent/models" => {
            agent_cmds::handle_models(
                req.id,
                req.params,
                agent_service.clone(),
                service_context.clone(),
            )
            .await
        }
        "agent/set-model" => {
            agent_cmds::handle_set_model(req.id, req.params, agent_service.clone()).await
        }
        "agent/usage" => {
            agent_cmds::handle_usage(req.id, req.params, service_context.clone()).await
        }
        "agent/clear-context" => {
            agent_cmds::handle_clear(req.id, req.params, agent_service.clone()).await
        }
        "agent/compact" => agent_cmds::handle_compact(req.id, req.params).await,
        "agent/plan" => agent_cmds::handle_plan(req.id, req.params).await,
        "agent/plan-show" => agent_cmds::handle_plan_show(req.id, req.params).await,
        "agent/plan-execute" => {
            agent_cmds::handle_plan_execute(req.id, req.params, agent_service.clone()).await
        }
        "agent/plan-discard" => {
            agent_cmds::handle_plan_discard(req.id, req.params, service_context.clone()).await
        }
        "agent/goal-check" => agent_cmds::handle_goal_check(req.id, req.params).await,
        _ => JsonRpcResponse::error(
            req.id,
            error_codes::METHOD_NOT_FOUND,
            format!("Method not found: {}", req.method),
        ),
    }
}
