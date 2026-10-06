//! Shared state types for the ACP server, split out of `server.rs` so the
//! dispatcher stays under the file-size ceiling: `SessionState` (one per
//! attached session), `ServerState` (one per process), and the steering
//! queue map they share with the agent service.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::protocol;
use super::transport::TransportHandle;
use crate::brain::agent::{AgentService, QueuedUserMessage};
use crate::services::{MessageService, SessionService};

/// Per-session steering queue (`session/steer`): drained by the tool loop's
/// `MessageQueueCallback` between iterations. Shared with the agent service
/// builder, which is why it lives outside the server struct's Mutex.
pub type SteerMap = Arc<Mutex<HashMap<Uuid, VecDeque<QueuedUserMessage>>>>;

pub fn new_steer_map() -> SteerMap {
    Arc::new(Mutex::new(HashMap::new()))
}

/// One ACP session's live state.
pub struct SessionState {
    /// The opencrabs session — same UUID the ACP session id stringifies.
    pub id: Uuid,
    /// Model override from `--model` or `session/set_model`/`session/set_mode`.
    pub model: Mutex<Option<String>>,
    /// Permission policy from `session/set_mode` (default supervised).
    pub mode: Mutex<protocol::AcpMode>,
    /// Cancel token of the in-flight turn; None when idle.
    pub active_cancel: Mutex<Option<CancellationToken>>,
    /// Cancels this state's cross-surface mirror watcher. Fired when a
    /// reload supersedes the state, so exactly one watcher per session.
    pub watch_cancel: CancellationToken,
}

/// Everything a dispatch or turn task needs, shared under one Arc.
pub struct ServerState {
    pub handle: TransportHandle,
    pub agent: Arc<AgentService>,
    pub sessions: SessionService,
    pub messages: MessageService,
    pub states: Mutex<HashMap<String, Arc<SessionState>>>,
    pub steer: SteerMap,
    pub default_model: Option<String>,
    pub config: Arc<crate::config::Config>,
    /// Client self-identification captured at `initialize`
    /// (`clientInfo.name`), used to compose descriptive birth titles.
    pub client_name: StdMutex<Option<String>>,
}
