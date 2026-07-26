use opencrabs_desktop_ui::bridge::{invoke, invoke_unit};
use opencrabs_desktop_ui::models::*;

use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use serde::{Deserialize, Serialize};
use serde_json::json;

const NAV_ITEMS: &[NavItem] = &[
    NavItem {
        id: RouteId::Chat,
        icon: "💬",
    },
    NavItem {
        id: RouteId::Files,
        icon: "📁",
    },
    NavItem {
        id: RouteId::Brain,
        icon: "🧠",
    },
    NavItem {
        id: RouteId::Providers,
        icon: "⚙️",
    },
    NavItem {
        id: RouteId::Tools,
        icon: "🛠",
    },
    NavItem {
        id: RouteId::Skills,
        icon: "✨",
    },
    NavItem {
        id: RouteId::Cron,
        icon: "⏱",
    },
    NavItem {
        id: RouteId::Channels,
        icon: "📡",
    },
    NavItem {
        id: RouteId::Usage,
        icon: "📊",
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RouteId {
    Chat,
    Files,
    Brain,
    Providers,
    Tools,
    Skills,
    Cron,
    Channels,
    Usage,
}

impl RouteId {
    fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Files => "files",
            Self::Brain => "brain",
            Self::Providers => "providers",
            Self::Tools => "tools",
            Self::Skills => "skills",
            Self::Cron => "cron",
            Self::Channels => "channels",
            Self::Usage => "usage",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "chat" => Some(Self::Chat),
            "files" => Some(Self::Files),
            "brain" => Some(Self::Brain),
            "providers" => Some(Self::Providers),
            "tools" => Some(Self::Tools),
            "skills" => Some(Self::Skills),
            "cron" => Some(Self::Cron),
            "channels" => Some(Self::Channels),
            "usage" => Some(Self::Usage),
            _ => None,
        }
    }
}

struct NavItem {
    id: RouteId,
    icon: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ActionStatus {
    scope: &'static str,
    message: String,
}

impl ActionStatus {
    fn new(scope: &'static str, message: impl Into<String>) -> Self {
        Self {
            scope,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct StreamState {
    active: bool,
    session_id: Option<String>,
    pending_text: String,
    pending_message_id: Option<String>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Debug, Deserialize, Serialize)]
struct StreamChunkPayload {
    text: String,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Debug, Deserialize, Serialize)]
struct StreamDonePayload {
    message_id: String,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Debug, Deserialize, Serialize)]
struct StreamStoppedPayload {
    session_id: String,
    message_id: String,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Debug, Deserialize, Serialize)]
struct StreamErrorPayload {
    error: String,
}

fn upsert_action_status(items: &mut Vec<ActionStatus>, next: ActionStatus) {
    if let Some(existing) = items.iter_mut().find(|item| item.scope == next.scope) {
        *existing = next;
    } else {
        items.push(next);
    }
}

fn clear_action_status(items: &mut Vec<ActionStatus>, scope: &'static str) {
    items.retain(|item| item.scope != scope);
}

fn push_warning_signal(signal: &mut Signal<Vec<String>>, message: String) {
    signal.with_mut(|items| items.push(message));
}

fn set_action_error(signal: &mut Signal<Vec<ActionStatus>>, scope: &'static str, message: String) {
    signal.with_mut(|items| upsert_action_status(items, ActionStatus::new(scope, message)));
}

fn clear_action_error(signal: &mut Signal<Vec<ActionStatus>>, scope: &'static str) {
    signal.with_mut(|items| clear_action_status(items, scope));
}

#[cfg(target_arch = "wasm32")]
fn subscribe_to_chat_events(
    stream_state: Signal<StreamState>,
    session_messages: Signal<Vec<MessageInfo>>,
    action_errors: Signal<Vec<ActionStatus>>,
) {
    use opencrabs_desktop_ui::bridge::{event_payload, listen};

    let mut chunks = stream_state;
    let mut chunk_errors = action_errors;
    spawn(async move {
        let result = listen("stream-chunk", move |event| {
            match event_payload::<StreamChunkPayload>(event) {
                Ok(payload) => chunks.with_mut(|stream| {
                    if stream.active {
                        stream.pending_text.push_str(&payload.text);
                    }
                }),
                Err(error) => set_action_error(&mut chunk_errors, "chat-stream", error),
            }
        })
        .await;
        if let Err(error) = result {
            set_action_error(
                &mut chunk_errors,
                "chat-stream",
                format!("Stream listener failed: {error}"),
            );
        }
    });

    let done_stream = stream_state;
    let done_messages = session_messages;
    let mut done_errors = action_errors;
    spawn(async move {
        let result = listen("stream-done", move |event| {
            if event_payload::<StreamDonePayload>(event).is_err() {
                return;
            }
            let Some(session_id) = done_stream.read().session_id.clone() else {
                return;
            };
            let mut stream = done_stream;
            let mut messages = done_messages;
            let mut errors = done_errors;
            spawn(async move {
                match invoke::<Vec<MessageInfo>, _>(
                    "get_session_messages",
                    json!({"sessionId": session_id}),
                )
                .await
                {
                    Ok(items) => {
                        messages.set(items);
                        stream.set(StreamState::default());
                        clear_action_error(&mut errors, "chat-stream");
                    }
                    Err(error) => set_action_error(
                        &mut errors,
                        "chat-stream",
                        format!("Stream refresh failed: {error}"),
                    ),
                }
            });
        })
        .await;
        if let Err(error) = result {
            set_action_error(
                &mut done_errors,
                "chat-stream",
                format!("Completion listener failed: {error}"),
            );
        }
    });

    let mut stopped_stream = stream_state;
    let mut stopped_errors = action_errors;
    spawn(async move {
        let result = listen("stream-stopped", move |event| {
            match event_payload::<StreamStoppedPayload>(event) {
                Ok(_) => stopped_stream.set(StreamState::default()),
                Err(error) => set_action_error(&mut stopped_errors, "chat-stop", error),
            }
        })
        .await;
        if let Err(error) = result {
            set_action_error(
                &mut stopped_errors,
                "chat-stop",
                format!("Stop listener failed: {error}"),
            );
        }
    });

    let mut errored_stream = stream_state;
    let mut errors = action_errors;
    spawn(async move {
        let result = listen("stream-error", move |event| {
            match event_payload::<StreamErrorPayload>(event) {
                Ok(payload) => {
                    errored_stream.set(StreamState::default());
                    set_action_error(&mut errors, "chat-stream", payload.error);
                }
                Err(error) => set_action_error(&mut errors, "chat-stream", error),
            }
        })
        .await;
        if let Err(error) = result {
            set_action_error(
                &mut errors,
                "chat-stream",
                format!("Error listener failed: {error}"),
            );
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn subscribe_to_chat_events(
    _stream_state: Signal<StreamState>,
    _session_messages: Signal<Vec<MessageInfo>>,
    _action_errors: Signal<Vec<ActionStatus>>,
) {
}

#[component]
pub fn App() -> Element {
    let mut route = use_signal(|| RouteId::Chat);
    let mut sessions = use_signal(Vec::<SessionInfo>::new);
    let mut selected_session = use_signal(|| None::<String>);
    let mut session_messages = use_signal(Vec::<MessageInfo>::new);
    let mut composer = use_signal(String::new);
    let workspace_root = use_signal(String::new);
    let current_directory = use_signal(String::new);
    let file_entries = use_signal(Vec::<FileEntry>::new);
    let selected_file = use_signal(|| None::<String>);
    let file_content = use_signal(|| None::<FileContent>);
    let brain_files = use_signal(Vec::<BrainFile>::new);
    let mut active_brain_tab = use_signal(|| None::<String>);
    let mut brain_editor = use_signal(String::new);
    let config_info = use_signal(|| None::<ConfigInfo>);
    let tools = use_signal(Vec::<ToolInfo>::new);
    let selected_tool = use_signal(|| None::<ToolDetail>);
    let skills = use_signal(Vec::<SkillInfo>::new);
    let selected_skill = use_signal(|| None::<SkillDetail>);
    let cron_jobs = use_signal(Vec::<CronJobInfo>::new);
    let cron_runs = use_signal(Vec::<CronJobRunInfo>::new);
    let channels = use_signal(Vec::<ChannelStatus>::new);
    let usage = use_signal(|| None::<DashboardDataInfo>);
    let status = use_signal(|| "Loading…".to_string());
    let error = use_signal(|| None::<String>);
    let background_warnings = use_signal(Vec::<String>::new);
    let mut action_errors = use_signal(Vec::<ActionStatus>::new);
    let mut stream_state = use_signal(StreamState::default);
    use_effect(move || subscribe_to_chat_events(stream_state, session_messages, action_errors));

    use_future(move || {
        let mut route = route;
        let mut sessions = sessions;
        let mut selected_session = selected_session;
        let mut session_messages = session_messages;
        let mut status = status;
        let mut error = error;
        let mut background_warnings = background_warnings;
        let mut workspace_root = workspace_root;
        let mut current_directory = current_directory;
        let mut file_entries = file_entries;
        let mut selected_file = selected_file;
        let mut file_content = file_content;
        let mut brain_files = brain_files;
        let mut active_brain_tab = active_brain_tab;
        let mut brain_editor = brain_editor;
        let mut config_info = config_info;
        let mut tools = tools;
        let mut skills = skills;
        let mut cron_jobs = cron_jobs;
        let mut channels = channels;
        let mut usage = usage;
        let mut stream_state = stream_state;

        async move {
            status.set("Loading sessions…".to_string());
            error.set(None);
            stream_state.set(StreamState::default());

            let persisted = invoke::<DesktopState, _>("get_desktop_state", json!({}))
                .await
                .unwrap_or(DesktopState {
                    route: RouteId::Chat.as_str().to_string(),
                    selected_session_id: None,
                });
            route.set(RouteId::parse(&persisted.route).unwrap_or(RouteId::Chat));

            match invoke::<Vec<SessionInfo>, _>("list_sessions", json!({})).await {
                Ok(list) => {
                    let persisted_id = persisted
                        .selected_session_id
                        .filter(|id| list.iter().any(|session| &session.id == id));
                    let session_id =
                        persisted_id.or_else(|| list.first().map(|session| session.id.clone()));
                    sessions.set(list);
                    selected_session.set(session_id.clone());
                    if let Some(session_id) = session_id {
                        match invoke::<Vec<MessageInfo>, _>(
                            "get_session_messages",
                            json!({"sessionId": session_id}),
                        )
                        .await
                        {
                            Ok(messages) => session_messages.set(messages),
                            Err(message) => error.set(Some(message)),
                        }
                    }
                    status.set("Ready".to_string());
                }
                Err(message) => {
                    status.set("Sessions unavailable".to_string());
                    error.set(Some(message));
                }
            }

            spawn(async move {
                let mut warnings = Vec::new();
                if let Ok(root) = invoke::<String, _>("get_workspace_root", json!({})).await {
                    workspace_root.set(root.clone());
                    current_directory.set(root.clone());
                    match invoke::<Vec<FileEntry>, _>("list_directory", json!({"path": root})).await
                    {
                        Ok(entries) => {
                            if let Some(first_file) = entries.iter().find(|entry| !entry.is_dir) {
                                selected_file.set(Some(first_file.path.clone()));
                                if let Ok(content) = invoke::<FileContent, _>(
                                    "read_file_content",
                                    json!({"path": first_file.path.clone()}),
                                )
                                .await
                                {
                                    file_content.set(Some(content));
                                }
                            }
                            file_entries.set(entries);
                        }
                        Err(message) => warnings.push(format!("Files unavailable: {message}")),
                    }
                } else {
                    warnings.push("Workspace root unavailable".to_string());
                }
                if !warnings.is_empty() {
                    background_warnings.with_mut(|items| items.extend(warnings));
                }
            });

            spawn(async move {
                match invoke::<Vec<BrainFile>, _>("list_brain_files", json!({})).await {
                    Ok(files) => {
                        if let Some(first) = files.first() {
                            active_brain_tab.set(Some(first.name.clone()));
                            brain_editor.set(first.content.clone());
                        }
                        brain_files.set(files);
                    }
                    Err(message) => push_warning_signal(
                        &mut background_warnings,
                        format!("Brain files unavailable: {message}"),
                    ),
                }
            });

            spawn(async move {
                match invoke::<ConfigInfo, _>("get_config", json!({})).await {
                    Ok(cfg) => config_info.set(Some(cfg)),
                    Err(message) => push_warning_signal(
                        &mut background_warnings,
                        format!("Config unavailable: {message}"),
                    ),
                }
            });

            spawn(async move {
                match invoke::<Vec<ToolInfo>, _>("list_tools", json!({})).await {
                    Ok(list) => tools.set(list),
                    Err(message) => push_warning_signal(
                        &mut background_warnings,
                        format!("Tools unavailable: {message}"),
                    ),
                }
            });

            spawn(async move {
                match invoke::<Vec<SkillInfo>, _>("list_skills", json!({})).await {
                    Ok(list) => skills.set(list),
                    Err(message) => push_warning_signal(
                        &mut background_warnings,
                        format!("Skills unavailable: {message}"),
                    ),
                }
            });

            spawn(async move {
                match invoke::<Vec<CronJobInfo>, _>("list_cron_jobs", json!({})).await {
                    Ok(list) => cron_jobs.set(list),
                    Err(message) => push_warning_signal(
                        &mut background_warnings,
                        format!("Cron unavailable: {message}"),
                    ),
                }
            });

            spawn(async move {
                match invoke::<Vec<ChannelStatus>, _>("get_channel_statuses", json!({})).await {
                    Ok(list) => channels.set(list),
                    Err(message) => push_warning_signal(
                        &mut background_warnings,
                        format!("Channels unavailable: {message}"),
                    ),
                }
            });

            spawn(async move {
                match invoke::<DashboardDataInfo, _>("get_usage_data", json!({"period": "week"}))
                    .await
                {
                    Ok(value) => usage.set(Some(value)),
                    Err(message) => push_warning_signal(
                        &mut background_warnings,
                        format!("Usage unavailable: {message}"),
                    ),
                }
            });
        }
    });

    let current_session_title = sessions
        .read()
        .iter()
        .find(|session| Some(&session.id) == selected_session.read().as_ref())
        .map(|s| s.title.clone())
        .unwrap_or_else(|| "No session selected".to_string());

    let rendered_messages = {
        let mut items = session_messages.read().clone();
        let active_stream = stream_state.read().clone();
        if active_stream.active && !active_stream.pending_text.is_empty() {
            let next_sequence = items.last().map(|m| m.sequence + 1).unwrap_or(1);
            items.push(MessageInfo {
                id: active_stream
                    .pending_message_id
                    .clone()
                    .unwrap_or_else(|| format!("streaming-assistant-{next_sequence}")),
                role: "assistant".to_string(),
                content: active_stream.pending_text.clone(),
                sequence: next_sequence,
                token_count: None,
                cost: None,
                created_at: "streaming".to_string(),
            });
        }
        items
    };

    rsx! {
        div { class: "app-shell",
            header { class: "topbar",
                div { class: "brand",
                    div { class: "brand-mark", "🦀" }
                    div {
                        h1 { "OpenCrabs Desktop" }
                        p { "Dioxus + Tauri command center" }
                    }
                }
                div { class: "top-actions",
                    span { class: "badge", "Provider status wired" }
                    span { class: "badge success", "{status.read()}" }
                }
            }

            nav { class: "tabs",
                for item in NAV_ITEMS {
                    button {
                        class: if *route.read() == item.id { "tab active" } else { "tab" },
                        onclick: {
                            let next_route = item.id;
                            move |_| {
                                route.set(next_route);
                                let state = DesktopState {
                                    route: next_route.as_str().to_string(),
                                    selected_session_id: selected_session.read().clone(),
                                };
                                spawn(async move {
                                    match invoke_unit("save_desktop_state", json!({"state": state})).await {
                                        Ok(()) => clear_action_error(&mut action_errors, "desktop-state"),
                                        Err(message) => set_action_error(
                                            &mut action_errors,
                                            "desktop-state",
                                            format!("Failed to save desktop view: {message}"),
                                        ),
                                    }
                                });
                            }
                        },
                        "{item.icon}"
                    }
                }
            }

            aside { class: "sidebar",
                div { class: "sidebar-header",
                    h2 { "Sessions" }
                    button {
                        class: "btn-new",
                        onclick: move |_| {
                            spawn(async move {
                                let created = invoke::<SessionInfo, _>("create_session", json!({"title": "New desktop session"})).await;
                                match created {
                                    Ok(session) => {
                                        sessions.with_mut(|list| list.insert(0, session.clone()));
                                        selected_session.set(Some(session.id.clone()));
                                        stream_state.set(StreamState::default());
                                        let state = DesktopState {
                                            route: route.read().as_str().to_string(),
                                            selected_session_id: Some(session.id.clone()),
                                        };
                                        match invoke_unit("save_desktop_state", json!({"state": state})).await {
                                            Ok(()) => clear_action_error(&mut action_errors, "desktop-state"),
                                            Err(message) => set_action_error(
                                                &mut action_errors,
                                                "desktop-state",
                                                format!("Failed to save selected session: {message}"),
                                            ),
                                        }
                                        match invoke::<Vec<MessageInfo>, _>(
                                            "get_session_messages",
                                            json!({"sessionId": session.id.clone()}),
                                        )
                                        .await
                                        {
                                            Ok(messages) => session_messages.set(messages),
                                            Err(message) => set_action_error(
                                                &mut action_errors,
                                                "sessions-create",
                                                format!("Session created, but messages could not load: {message}"),
                                            ),
                                        }
                                    }
                                    Err(message) => set_action_error(
                                        &mut action_errors,
                                        "sessions-create",
                                        format!("Failed to create session: {message}"),
                                    ),
                                }
                            });
                        },
                        "+ New"
                    }
                }
                div { class: "sidebar-nav",
                    span { class: "nav-label", "Workspace" }
                    div { class: "nav-item", "📍 {workspace_root.read()}" }
                    span { class: "nav-label", "Status" }
                    div { class: "nav-item", "{status.read()}" }
                }
                div { class: "session-list",
                    for session in sessions.read().iter() {
                        button {
                            class: if Some(&session.id) == selected_session.read().as_ref() { "session-card active" } else { "session-card" },
                            onclick: {
                                let session_id = session.id.clone();
                                move |_| {
                                    let id_for_messages = session_id.clone();
                                    let id_for_state = session_id.clone();
                                    selected_session.set(Some(session_id.clone()));
                                    stream_state.set(StreamState::default());
                                    let state = DesktopState {
                                        route: route.read().as_str().to_string(),
                                        selected_session_id: Some(id_for_state),
                                    };
                                    spawn(async move {
                                        match invoke_unit("save_desktop_state", json!({"state": state})).await {
                                            Ok(()) => clear_action_error(&mut action_errors, "desktop-state"),
                                            Err(message) => set_action_error(
                                                &mut action_errors,
                                                "desktop-state",
                                                format!("Failed to save selected session: {message}"),
                                            ),
                                        }
                                        match invoke::<Vec<MessageInfo>, _>(
                                            "get_session_messages",
                                            json!({"sessionId": id_for_messages}),
                                        )
                                        .await
                                        {
                                            Ok(messages) => {
                                                session_messages.set(messages);
                                                clear_action_error(&mut action_errors, "sessions-load");
                                            }
                                            Err(message) => set_action_error(
                                                &mut action_errors,
                                                "sessions-load",
                                                format!("Failed to load selected session: {message}"),
                                            ),
                                        }
                                    });
                                }
                            },
                            div { class: "session-title", "{session.title}" }
                        }
                    }
                }
            }

            main { class: "panel-container",
                if let Some(err) = error.read().clone() {
                    div { class: "card", style: "border-color: var(--text-error); margin-bottom: 16px;",
                        h3 { "Frontend bridge error" }
                        p { "{err}" }
                        p { class: "subtle", "Check Usage → Diagnostics for a redacted local support snapshot." }
                    }
                }
                if !background_warnings.read().is_empty() {
                    div { class: "card", style: "margin-bottom: 16px;",
                        h3 { "Some desktop panels are unavailable" }
                        for warning in background_warnings.read().iter() {
                            p { class: "subtle", "{warning}" }
                        }
                    }
                }
                if !action_errors.read().is_empty() {
                    div { class: "card", style: "border-color: var(--text-error); margin-bottom: 16px;",
                        h3 { "Desktop actions need attention" }
                        for item in action_errors.read().iter() {
                            p { class: "subtle", "{item.scope}: {item.message}" }
                        }
                    }
                }

                match *route.read() {
                    RouteId::Chat => rsx! {
                        ChatPanel {
                            title: current_session_title,
                            messages: rendered_messages,
                            composer: composer.read().clone(),
                            is_streaming: stream_state.read().active,
                            on_input: move |value| composer.set(value),
                            on_send: move |_| {
                                let maybe_session = selected_session.read().clone();
                                let text = composer.read().trim().to_string();
                                if text.is_empty() {
                                    return;
                                }
                                if let Some(session_id) = maybe_session {
                                    composer.set(String::new());
                                    session_messages.with_mut(|msgs| {
                                        let next_sequence = msgs.last().map(|m| m.sequence + 1).unwrap_or(1);
                                        msgs.push(MessageInfo {
                                            id: format!("local-user-{next_sequence}"),
                                            role: "user".to_string(),
                                            content: text.clone(),
                                            sequence: next_sequence,
                                            token_count: None,
                                            cost: None,
                                            created_at: "now".to_string(),
                                        });
                                    });
                                    stream_state.set(StreamState {
                                        active: true,
                                        session_id: Some(session_id.clone()),
                                        pending_text: String::new(),
                                        pending_message_id: None,
                                    });
                                    clear_action_error(&mut action_errors, "chat-send");
                                    spawn(async move {
                                        match invoke::<String, _>(
                                            "send_message_streaming",
                                            json!({"sessionId": session_id.clone(), "message": text, "model": null}),
                                        )
                                        .await
                                        {
                                            Ok(message_id) => stream_state.with_mut(|stream| {
                                                stream.active = true;
                                                stream.session_id = Some(session_id.clone());
                                                stream.pending_message_id = Some(message_id);
                                            }),
                                            Err(message) => {
                                                stream_state.set(StreamState::default());
                                                set_action_error(
                                                    &mut action_errors,
                                                    "chat-send",
                                                    format!("Failed to send chat message: {message}"),
                                                );
                                            }
                                        }
                                    });
                                }
                            },
                            on_stop: move |_| {
                                if let Some(session_id) = selected_session.read().clone() {
                                    spawn(async move {
                                        match invoke_unit("stop_generation", json!({"sessionId": session_id.clone()})).await {
                                            Ok(()) => clear_action_error(&mut action_errors, "chat-stop"),
                                            Err(message) => set_action_error(
                                                &mut action_errors,
                                                "chat-stop",
                                                format!("Failed to stop generation for {session_id}: {message}"),
                                            ),
                                        }
                                    });
                                }
                            }
                        }
                    },
                    RouteId::Files => rsx! {
                        FilesPanel {
                            root: workspace_root.read().clone(),
                            current_directory: current_directory.read().clone(),
                            entries: file_entries.read().clone(),
                            selected_path: selected_file.read().clone(),
                            preview: file_content.read().clone(),
                            on_open: move |entry: FileEntry| {
                                let mut file_entries = file_entries;
                                let mut current_directory = current_directory;
                                let mut selected_file = selected_file;
                                let mut file_content = file_content;
                                let mut action_errors = action_errors;
                                spawn(async move {
                                    if entry.is_dir {
                                        match invoke::<Vec<FileEntry>, _>("list_directory", json!({"path": entry.path.clone()})).await {
                                            Ok(entries) => {
                                                clear_action_error(&mut action_errors, "files-open");
                                                current_directory.set(entry.path.clone());
                                                selected_file.set(None);
                                                file_content.set(None);
                                                file_entries.set(entries);
                                            }
                                            Err(message) => set_action_error(
                                                &mut action_errors,
                                                "files-open",
                                                format!("Failed to open directory {}: {message}", entry.path),
                                            ),
                                        }
                                    } else {
                                        selected_file.set(Some(entry.path.clone()));
                                        match invoke::<FileContent, _>("read_file_content", json!({"path": entry.path.clone()})).await {
                                            Ok(content) => {
                                                clear_action_error(&mut action_errors, "files-open");
                                                file_content.set(Some(content));
                                            }
                                            Err(message) => set_action_error(
                                                &mut action_errors,
                                                "files-open",
                                                format!("Failed to read file {}: {message}", entry.path),
                                            ),
                                        }
                                    }
                                });
                            },
                            on_up: move |_| {
                                let current = current_directory.read().clone();
                                let root = workspace_root.read().clone();
                                if current == root || current.is_empty() {
                                    return;
                                }
                                let mut file_entries = file_entries;
                                let mut current_directory = current_directory;
                                let mut selected_file = selected_file;
                                let mut file_content = file_content;
                                let mut action_errors = action_errors;
                                spawn(async move {
                                    let parent = std::path::Path::new(&current)
                                        .parent()
                                        .map(|path| path.to_string_lossy().to_string())
                                        .unwrap_or(root.clone());
                                    match invoke::<Vec<FileEntry>, _>("list_directory", json!({"path": parent.clone()})).await {
                                        Ok(entries) => {
                                            clear_action_error(&mut action_errors, "files-up");
                                            current_directory.set(parent);
                                            selected_file.set(None);
                                            file_content.set(None);
                                            file_entries.set(entries);
                                        }
                                        Err(message) => set_action_error(
                                            &mut action_errors,
                                            "files-up",
                                            format!("Failed to open parent directory: {message}"),
                                        ),
                                    }
                                });
                            }
                        }
                    },
                    RouteId::Brain => rsx! {
                        BrainPanel {
                            files: brain_files.read().clone(),
                            active_tab: active_brain_tab.read().clone(),
                            editor: brain_editor.read().clone(),
                            on_select: move |name: String| {
                                if let Some(file) = brain_files.read().iter().find(|file| file.name == name) {
                                    active_brain_tab.set(Some(file.name.clone()));
                                    brain_editor.set(file.content.clone());
                                }
                            },
                            on_edit: move |value| brain_editor.set(value),
                            on_save: move |_| {
                                let active = active_brain_tab.read().clone();
                                let content = brain_editor.read().clone();
                                if let Some(name) = active {
                                    let mut brain_files = brain_files;
                                    let mut action_errors = action_errors;
                                    let mut active_brain_tab = active_brain_tab;
                                    let mut brain_editor = brain_editor;
                                    spawn(async move {
                                        match invoke_unit("write_brain_file", json!({"name": name.clone(), "content": content.clone()})).await {
                                            Ok(()) => {
                                                clear_action_error(&mut action_errors, "brain-save");
                                                match invoke::<Vec<BrainFile>, _>("list_brain_files", json!({})).await {
                                                    Ok(files) => {
                                                        if let Some(file) = files.iter().find(|file| file.name == name) {
                                                            active_brain_tab.set(Some(file.name.clone()));
                                                            brain_editor.set(file.content.clone());
                                                        }
                                                        brain_files.set(files);
                                                    }
                                                    Err(message) => set_action_error(
                                                        &mut action_errors,
                                                        "brain-save",
                                                        format!("Brain file saved, but refresh failed: {message}"),
                                                    ),
                                                }
                                            }
                                            Err(message) => set_action_error(
                                                &mut action_errors,
                                                "brain-save",
                                                format!("Failed to save brain file {name}: {message}"),
                                            ),
                                        }
                                    });
                                }
                            }
                        }
                    },
                    RouteId::Providers => rsx! {
                        ProvidersPanel {
                            config: config_info.read().clone(),
                            on_select_model: move |(provider_name, model): (String, String)| {
                                let mut config_info = config_info;
                                let mut action_errors = action_errors;
                                spawn(async move {
                                    match invoke_unit(
                                        "select_model",
                                        json!({"providerName": provider_name.clone(), "model": model.clone()}),
                                    )
                                    .await
                                    {
                                        Ok(()) => {
                                            match invoke::<ConfigInfo, _>("get_config", json!({})).await {
                                                Ok(cfg) => {
                                                    config_info.set(Some(cfg));
                                                    clear_action_error(&mut action_errors, "providers-select");
                                                }
                                                Err(message) => set_action_error(
                                                    &mut action_errors,
                                                    "providers-select",
                                                    format!("Model selected, but provider refresh failed: {message}"),
                                                ),
                                            }
                                        }
                                        Err(message) => set_action_error(
                                            &mut action_errors,
                                            "providers-select",
                                            format!("Failed to select model for {provider_name}: {message}"),
                                        ),
                                    }
                                });
                            }
                        }
                    },
                    RouteId::Tools => rsx! {
                        ToolsPanel {
                            tools: tools.read().clone(),
                            selected: selected_tool.read().clone(),
                            on_open: move |tool_name: String| {
                                let mut selected_tool = selected_tool;
                                let mut action_errors = action_errors;
                                spawn(async move {
                                    match invoke::<ToolDetail, _>("get_tool_details", json!({"toolName": tool_name.clone()})).await {
                                        Ok(detail) => {
                                            clear_action_error(&mut action_errors, "tools-open");
                                            selected_tool.set(Some(detail));
                                        }
                                        Err(message) => set_action_error(
                                            &mut action_errors,
                                            "tools-open",
                                            format!("Tool details failed for {tool_name}: {message}"),
                                        ),
                                    }
                                });
                            },
                            on_approve: move |tool_name: String| {
                                let session_id = selected_session.read().clone().unwrap_or_else(|| "desktop".to_string());
                                let mut action_errors = action_errors;
                                spawn(async move {
                                    match invoke_unit(
                                        "approve_tool",
                                        json!({
                                            "sessionId": session_id,
                                            "toolName": tool_name.clone(),
                                            "approved": true,
                                            "alwaysApprove": false
                                        }),
                                    )
                                    .await
                                    {
                                        Ok(()) => clear_action_error(&mut action_errors, "tools-approve"),
                                        Err(message) => set_action_error(
                                            &mut action_errors,
                                            "tools-approve",
                                            format!("Failed to update approval for {tool_name}: {message}"),
                                        ),
                                    }
                                });
                            }
                        }
                    },
                    RouteId::Skills => rsx! {
                        SkillsPanel {
                            skills: skills.read().clone(),
                            selected: selected_skill.read().clone(),
                            on_open: move |name: String| {
                                let mut selected_skill = selected_skill;
                                let mut action_errors = action_errors;
                                spawn(async move {
                                    match invoke::<SkillDetail, _>("get_skill_details", json!({"name": name.clone()})).await {
                                        Ok(detail) => {
                                            clear_action_error(&mut action_errors, "skills-open");
                                            selected_skill.set(Some(detail));
                                        }
                                        Err(message) => set_action_error(
                                            &mut action_errors,
                                            "skills-open",
                                            format!("Skill details failed for {name}: {message}"),
                                        ),
                                    }
                                });
                            },
                            on_toggle: move |(name, enabled): (String, bool)| {
                                let mut skills = skills;
                                let mut selected_skill = selected_skill;
                                let mut action_errors = action_errors;
                                spawn(async move {
                                    match invoke_unit("toggle_skill", json!({"name": name.clone(), "enabled": enabled})).await {
                                        Ok(()) => {
                                            match invoke::<Vec<SkillInfo>, _>("list_skills", json!({})).await {
                                                Ok(list) => skills.set(list),
                                                Err(message) => {
                                                    set_action_error(
                                                        &mut action_errors,
                                                        "skills-toggle",
                                                        format!("Skill changed, but list refresh failed: {message}"),
                                                    );
                                                    return;
                                                }
                                            }
                                            match invoke::<SkillDetail, _>("get_skill_details", json!({"name": name.clone()})).await {
                                                Ok(detail) => {
                                                    selected_skill.set(Some(detail));
                                                    clear_action_error(&mut action_errors, "skills-toggle");
                                                }
                                                Err(message) => set_action_error(
                                                    &mut action_errors,
                                                    "skills-toggle",
                                                    format!("Skill changed, but detail refresh failed: {message}"),
                                                ),
                                            }
                                        }
                                        Err(message) => set_action_error(
                                            &mut action_errors,
                                            "skills-toggle",
                                            format!("Failed to toggle skill {name}: {message}"),
                                        ),
                                    }
                                });
                            }
                        }
                    },
                    RouteId::Cron => rsx! {
                        CronPanel {
                            jobs: cron_jobs.read().clone(),
                            runs: cron_runs.read().clone(),
                            on_toggle: move |(job_id, enabled): (String, bool)| {
                                let mut cron_jobs = cron_jobs;
                                let mut action_errors = action_errors;
                                spawn(async move {
                                    match invoke_unit("toggle_cron_job", json!({"jobId": job_id.clone(), "enabled": enabled})).await {
                                        Ok(()) => match invoke::<Vec<CronJobInfo>, _>("list_cron_jobs", json!({})).await {
                                            Ok(list) => { cron_jobs.set(list); clear_action_error(&mut action_errors, "cron-toggle"); }
                                            Err(message) => set_action_error(&mut action_errors, "cron-toggle", format!("Cron changed, but refresh failed: {message}")),
                                        },
                                        Err(message) => set_action_error(&mut action_errors, "cron-toggle", format!("Failed to toggle cron job {job_id}: {message}")),
                                    }
                                });
                            },
                            on_run_now: move |job_id: String| {
                                let mut cron_runs = cron_runs;
                                let mut action_errors = action_errors;
                                spawn(async move {
                                    match invoke_unit("trigger_cron_job", json!({"jobId": job_id.clone()})).await {
                                        Ok(()) => match invoke::<Vec<CronJobRunInfo>, _>("list_cron_runs", json!({"jobId": job_id})).await {
                                            Ok(runs) => { cron_runs.set(runs); clear_action_error(&mut action_errors, "cron-run"); }
                                            Err(message) => set_action_error(&mut action_errors, "cron-run", format!("Cron triggered, but history refresh failed: {message}")),
                                        },
                                        Err(message) => set_action_error(&mut action_errors, "cron-run", format!("Failed to trigger cron job {job_id}: {message}")),
                                    }
                                });
                            },
                            on_show_runs: move |job_id: String| {
                                let mut cron_runs = cron_runs;
                                let mut action_errors = action_errors;
                                spawn(async move {
                                    match invoke::<Vec<CronJobRunInfo>, _>("list_cron_runs", json!({"jobId": job_id.clone()})).await {
                                        Ok(runs) => { cron_runs.set(runs); clear_action_error(&mut action_errors, "cron-runs"); }
                                        Err(message) => set_action_error(&mut action_errors, "cron-runs", format!("Failed to load runs for {job_id}: {message}")),
                                    }
                                });
                            },
                            on_delete: move |job_id: String| {
                                let mut cron_jobs = cron_jobs;
                                let mut cron_runs = cron_runs;
                                let mut action_errors = action_errors;
                                spawn(async move {
                                    match invoke_unit("delete_cron_job", json!({"jobId": job_id.clone()})).await {
                                        Ok(()) => match invoke::<Vec<CronJobInfo>, _>("list_cron_jobs", json!({})).await {
                                            Ok(list) => { cron_jobs.set(list); cron_runs.set(Vec::new()); clear_action_error(&mut action_errors, "cron-delete"); }
                                            Err(message) => set_action_error(&mut action_errors, "cron-delete", format!("Cron job deleted, but refresh failed: {message}")),
                                        },
                                        Err(message) => set_action_error(&mut action_errors, "cron-delete", format!("Failed to delete cron job {job_id}: {message}")),
                                    }
                                });
                            }
                        }
                    },
                    RouteId::Channels => rsx! {
                        ChannelsPanel {
                            channels: channels.read().clone(),
                            on_toggle: move |(name, enabled): (String, bool)| {
                                let mut channels = channels;
                                let mut action_errors = action_errors;
                                spawn(async move {
                                    match invoke_unit("toggle_channel", json!({"name": name.clone(), "enabled": enabled})).await {
                                        Ok(()) => match invoke::<Vec<ChannelStatus>, _>("get_channel_statuses", json!({})).await {
                                            Ok(list) => {
                                                channels.set(list);
                                                clear_action_error(&mut action_errors, "channels-toggle");
                                            }
                                            Err(message) => set_action_error(
                                                &mut action_errors,
                                                "channels-toggle",
                                                format!("Channel changed, but status refresh failed: {message}"),
                                            ),
                                        },
                                        Err(message) => set_action_error(
                                            &mut action_errors,
                                            "channels-toggle",
                                            format!("Failed to toggle channel {name}: {message}"),
                                        ),
                                    }
                                });
                            }
                        }
                    },
                    RouteId::Usage => rsx! {
                        UsagePanel { data: usage.read().clone() }
                        DiagnosticsPanel {}
                    },
                }
            }
        }
    }
}

#[component]
fn ChatPanel(
    title: String,
    messages: Vec<MessageInfo>,
    composer: String,
    is_streaming: bool,
    on_input: EventHandler<String>,
    on_send: EventHandler<()>,
    on_stop: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "chat-panel",
            div { class: "chat-header",
                div { class: "chat-title", "{title}" }
                div { class: "card-actions",
                    button { class: "btn-small", disabled: !is_streaming, onclick: move |_| on_stop.call(()), "Stop" }
                    span { class: "badge", if is_streaming { "streaming" } else { "idle" } }
                }
            }
            div { class: "message-list",
                for message in messages.iter() {
                    div {
                        class: if message.role == "user" { "message-bubble user" } else { "message-bubble assistant" },
                        div { class: "msg-avatar", if message.role == "user" { "U" } else { "A" } }
                        div {
                            div { class: "msg-text", "{message.content}" }
                            div { class: "msg-meta", "{message.role} · {message.created_at}" }
                        }
                    }
                }
            }
            div { class: "input-bar",
                textarea {
                    class: "chat-input",
                    value: "{composer}",
                    placeholder: "Talk to OpenCrabs…",
                    oninput: move |evt| on_input.call(evt.value()),
                }
                button { class: "btn-send", disabled: is_streaming, onclick: move |_| on_send.call(()), "➤" }
            }
        }
    }
}

#[component]
fn FilesPanel(
    root: String,
    current_directory: String,
    entries: Vec<FileEntry>,
    selected_path: Option<String>,
    preview: Option<FileContent>,
    on_open: EventHandler<FileEntry>,
    on_up: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "files-panel",
            div { class: "panel-header",
                h2 { "Workspace files" }
                code { "{root}" }
            }
            div { class: "card", style: "margin-bottom: 16px;",
                p { class: "subtle", "Workspace browsing is constrained to the active desktop workspace root." }
                p { class: "subtle", "Current directory: {current_directory}" }
                button { class: "btn-small", disabled: current_directory == root || current_directory.is_empty(), onclick: move |_| on_up.call(()), "Up one directory" }
            }
            div { class: "files-layout",
                div { class: "file-list card",
                    h3 { "Entries" }
                    for entry in entries.iter() {
                        button {
                            class: if selected_path.as_ref() == Some(&entry.path) { "session-card active" } else { "session-card" },
                            onclick: {
                                let entry = entry.clone();
                                move |_| on_open.call(entry.clone())
                            },
                            div { class: "session-title", "{entry.name}" }
                            if entry.is_dir {
                                div { class: "msg-meta", "directory" }
                            } else if let Some(size) = entry.size {
                                div { class: "msg-meta", "{size} bytes" }
                            }
                        }
                    }
                }
                div { class: "file-preview",
                    if let Some(path) = selected_path {
                        h3 { "{path}" }
                    }
                    if let Some(content) = preview {
                        pre { class: "code-block", "{content.content}" }
                    } else {
                        p { "Pick a file to preview." }
                    }
                }
            }
        }
    }
}

#[component]
fn BrainPanel(
    files: Vec<BrainFile>,
    active_tab: Option<String>,
    editor: String,
    on_select: EventHandler<String>,
    on_edit: EventHandler<String>,
    on_save: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "editor-layout",
            div { class: "panel-header",
                h2 { "Brain files" }
                button { class: "btn-primary", onclick: move |_| on_save.call(()), "Save" }
            }
            div { class: "card", style: "margin-bottom: 16px;",
                p { class: "subtle", "Protected brain files keep ownership headers and reject empty writes." }
            }
            div { class: "editor-tabs",
                for file in files.iter() {
                    button {
                        class: if active_tab.as_ref() == Some(&file.name) { "editor-tab active" } else { "editor-tab" },
                        onclick: {
                            let name = file.name.clone();
                            move |_| on_select.call(name.clone())
                        },
                        "{file.name}"
                    }
                }
            }
            textarea {
                class: "brain-textarea",
                value: "{editor}",
                oninput: move |evt| on_edit.call(evt.value()),
            }
        }
    }
}

#[component]
fn ProvidersPanel(
    config: Option<ConfigInfo>,
    on_select_model: EventHandler<(String, String)>,
) -> Element {
    rsx! {
        div {
            div { class: "panel-header",
                h2 { "Providers" }
                if let Some(cfg) = &config {
                    div { class: "badge key-ok", "Auto approve: {cfg.agent_auto_approve}" }
                }
            }
            if let Some(cfg) = config {
                div { class: "card-grid",
                    for provider in cfg.providers.iter() {
                        div { class: "card",
                            div { class: "card-body",
                                h3 { "{provider.name}" }
                                div { class: "card-tags",
                                    span { class: if provider.enabled { "badge enabled" } else { "badge disabled" }, if provider.enabled { "enabled" } else { "disabled" } }
                                    span { class: if provider.has_api_key { "badge key-ok" } else { "badge" }, if provider.has_api_key { "key present" } else { "no key" } }
                                }
                                if let Some(model) = provider.default_model.as_deref() {
                                    p { class: "subtle", "Default model: {model}" }
                                }
                                if !provider.models.is_empty() {
                                    div { class: "card-tags",
                                        for model in provider.models.iter().take(3) {
                                            button {
                                                class: "btn-small",
                                                onclick: {
                                                    let provider_name = provider.name.clone();
                                                    let model_name = model.clone();
                                                    move |_| on_select_model.call((provider_name.clone(), model_name.clone()))
                                                },
                                                "Use {model}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                p { "No provider data yet." }
            }
        }
    }
}

#[component]
fn ToolsPanel(
    tools: Vec<ToolInfo>,
    selected: Option<ToolDetail>,
    on_open: EventHandler<String>,
    on_approve: EventHandler<String>,
) -> Element {
    rsx! {
        div {
            div { class: "panel-header",
                h2 { "Tools" }
                span { class: "badge", "{tools.len()} loaded" }
            }
            div { class: "card", style: "margin-bottom: 16px;",
                p { class: "subtle", "Desktop tool approval persists policy, but does not yet mirror the TUI's richer inline approval event flow." }
            }
            div { class: "card-grid",
                for tool in tools.iter() {
                    button {
                        class: "card",
                        onclick: {
                            let name = tool.name.clone();
                            move |_| on_open.call(name.clone())
                        },
                        div { class: "card-body",
                            h3 { "{tool.name}" }
                            p { "{tool.description}" }
                            div { class: "card-tags",
                                for cap in tool.capabilities.iter() {
                                    span { class: "tag", "{cap}" }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(detail) = selected {
                div { class: "modal",
                    h2 { "{detail.name}" }
                    p { "{detail.description}" }
                    button {
                        class: "btn-primary",
                        onclick: {
                            let tool_name = detail.name.clone();
                            move |_| on_approve.call(tool_name.clone())
                        },
                        "Allow this tool for the current session"
                    }
                    pre { class: "code-block", "{detail.parameters}" }
                }
            }
        }
    }
}

#[component]
fn SkillsPanel(
    skills: Vec<SkillInfo>,
    selected: Option<SkillDetail>,
    on_open: EventHandler<String>,
    on_toggle: EventHandler<(String, bool)>,
) -> Element {
    rsx! {
        div {
            div { class: "panel-header",
                h2 { "Skills" }
                span { class: "badge", "{skills.len()} discovered" }
            }
            div { class: "card-grid",
                for skill in skills.iter() {
                    div { class: "card",
                        div { class: "card-body",
                            button {
                                class: "btn-small",
                                onclick: {
                                    let name = skill.name.clone();
                                    move |_| on_open.call(name.clone())
                                },
                                "Open"
                            }
                            h3 { "{skill.name}" }
                            p { "{skill.description}" }
                            div { class: "card-tags",
                                span { class: "badge", "{skill.source}" }
                                span { class: if skill.enabled { "badge enabled" } else { "badge disabled" }, if skill.enabled { "enabled" } else { "disabled" } }
                                if skill.review_gate {
                                    span { class: "badge review-gate", "review gate" }
                                }
                            }
                            button {
                                class: "btn-small",
                                onclick: {
                                    let name = skill.name.clone();
                                    let next = !skill.enabled;
                                    move |_| on_toggle.call((name.clone(), next))
                                },
                                if skill.enabled { "Disable" } else { "Enable" }
                            }
                        }
                    }
                }
            }
            if let Some(detail) = selected {
                div { class: "modal",
                    h2 { "{detail.name}" }
                    p { "{detail.description}" }
                    p { class: "subtle", "Enabled: {detail.enabled}" }
                    pre { class: "code-block", "{detail.body}" }
                }
            }
        }
    }
}

#[component]
fn CronPanel(
    jobs: Vec<CronJobInfo>,
    runs: Vec<CronJobRunInfo>,
    on_toggle: EventHandler<(String, bool)>,
    on_run_now: EventHandler<String>,
    on_show_runs: EventHandler<String>,
    on_delete: EventHandler<String>,
) -> Element {
    let mut pending_delete = use_signal(|| None::<String>);

    rsx! {
        div {
            div { class: "panel-header",
                h2 { "Cron jobs" }
                span { class: "badge", "{jobs.len()} scheduled" }
            }
            div { class: "card", style: "margin-bottom: 16px;",
                p { class: "subtle", "Run history is read-only. Deleting a job permanently removes its schedule." }
            }
            if jobs.is_empty() {
                div { class: "empty-state",
                    h3 { "No scheduled jobs" }
                    p { "Schedules created through OpenCrabs will appear here with their run history." }
                }
            } else {
                div { class: "table-container",
                    table { class: "data-table",
                        thead { tr { th { "Name" } th { "Schedule" } th { "Provider" } th { "Enabled" } th { "Actions" } } }
                        tbody {
                            for job in jobs.iter() {
                                tr {
                                    td { "{job.name}" }
                                    td { "{job.cron_expr}" }
                                    td { "{job.provider.as_deref().unwrap_or(\"default\")}" }
                                    td { if job.enabled { "yes" } else { "no" } }
                                    td {
                                        button { class: "btn-small", onclick: { let id = job.id.clone(); let next = !job.enabled; move |_| on_toggle.call((id.clone(), next)) }, if job.enabled { "Disable" } else { "Enable" } }
                                        button { class: "btn-small", onclick: { let id = job.id.clone(); move |_| on_run_now.call(id.clone()) }, "Run now" }
                                        button { class: "btn-small", onclick: { let id = job.id.clone(); move |_| on_show_runs.call(id.clone()) }, "History" }
                                        if pending_delete.read().as_deref() == Some(job.id.as_str()) {
                                            button { class: "btn-small danger", onclick: { let id = job.id.clone(); move |_| { on_delete.call(id.clone()); pending_delete.set(None); } }, "Confirm delete" }
                                            button { class: "btn-small", onclick: move |_| pending_delete.set(None), "Cancel" }
                                        } else {
                                            button { class: "btn-small danger", onclick: { let id = job.id.clone(); move |_| pending_delete.set(Some(id.clone())) }, "Delete…" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !runs.is_empty() {
                div { class: "card", style: "margin-top: 16px;",
                    h3 { "Recent runs" }
                    div { class: "table-container",
                        table { class: "data-table",
                            thead { tr { th { "Status" } th { "Started" } th { "Tokens" } th { "Error" } } }
                            tbody {
                                for run in runs.iter() {
                                    tr {
                                        td { "{run.status}" }
                                        td { "{run.started_at}" }
                                        td { "{run.input_tokens + run.output_tokens}" }
                                        td { "{run.error.as_deref().unwrap_or(\"—\")}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ChannelsPanel(channels: Vec<ChannelStatus>, on_toggle: EventHandler<(String, bool)>) -> Element {
    rsx! {
        div {
            div { class: "panel-header",
                h2 { "Channels" }
                span { class: "badge", "{channels.len()} integrations" }
            }
            div { class: "card", style: "margin-bottom: 16px;",
                p { class: "subtle", "Channel status reports configuration and credential readiness only. It does not prove a live socket or external service connection." }
            }
            div { class: "card-grid",
                for channel in channels.iter() {
                    div { class: "card",
                        div { class: "card-body",
                            h3 { "{channel.display_name}" }
                            div { class: "card-actions",
                                span {
                                    class: if channel.enabled { "badge enabled" } else { "badge disabled" },
                                    if channel.enabled { "enabled" } else { "disabled" }
                                }
                                span {
                                    class: if channel.alive { "badge key-ok" } else { "badge" },
                                    if channel.alive { "credentials present" } else { "not ready" }
                                }
                                button {
                                    class: "btn-small",
                                    onclick: {
                                        let name = channel.name.clone();
                                        let enabled = !channel.enabled;
                                        move |_| on_toggle.call((name.clone(), enabled))
                                    },
                                    if channel.enabled { "Disable" } else { "Enable" }
                                }
                            }
                            if let Some(err) = &channel.error {
                                p { class: "subtle", "{err}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DiagnosticsPanel() -> Element {
    let mut snapshot = use_signal(|| None::<DiagnosticsSnapshot>);
    let mut error = use_signal(|| None::<String>);

    let load = move |_| {
        spawn(async move {
            error.set(None);
            match invoke::<DiagnosticsSnapshot, _>("get_diagnostics", json!({})).await {
                Ok(value) => snapshot.set(Some(value)),
                Err(message) => error.set(Some(message)),
            }
        });
    };

    rsx! {
        div { class: "card", style: "margin-top: 16px;",
            div { class: "panel-header",
                h3 { "Diagnostics" }
                button { class: "btn-secondary", onclick: load, "Refresh diagnostics" }
            }
            p { class: "subtle", "Read-only support snapshot. Credential-bearing log lines are redacted." }
            if let Some(message) = error.read().clone() {
                p { class: "error", "Diagnostics unavailable: {message}" }
            }
            if let Some(value) = snapshot.read().clone() {
                p { class: "subtle", "App {value.app_version} · config: {value.config_present} · database: {value.database_present}" }
                p { class: "subtle", "Log: {value.log_path}" }
                for note in value.notes {
                    p { class: "subtle", "{note}" }
                }
                if value.log_tail.is_empty() {
                    p { class: "subtle", "No safe log entries available." }
                } else {
                    pre { class: "file-preview", "{value.log_tail.join(\"\\n\")}" }
                }
            } else {
                p { class: "subtle", "Refresh to collect a safe local diagnostic snapshot." }
            }
        }
    }
}

#[component]
fn UsagePanel(data: Option<DashboardDataInfo>) -> Element {
    rsx! {
        div {
            div { class: "panel-header",
                h2 { "Usage" }
                span { class: "badge", "week" }
            }
            if let Some(data) = data {
                div { class: "summary-grid",
                    SummaryCard { title: "Tokens", value: data.summary.total_tokens.to_string() }
                    SummaryCard { title: "Cost", value: format!("${:.2}", data.summary.total_cost) }
                    SummaryCard { title: "Sessions", value: data.summary.session_count.to_string() }
                    SummaryCard { title: "Calls", value: data.summary.call_count.to_string() }
                }
                div { class: "table-container",
                    table { class: "data-table",
                        thead {
                            tr {
                                th { "Project" }
                                th { "Tokens" }
                                th { "Cost" }
                            }
                        }
                        tbody {
                            for project in data.projects.iter() {
                                tr {
                                    td { "{project.project}" }
                                    td { class: "num", "{project.tokens}" }
                                    td { class: "num", "${project.cost}" }
                                }
                            }
                        }
                    }
                }
            } else {
                p { "Usage data unavailable." }
            }
        }
    }
}

#[component]
fn SummaryCard(title: String, value: String) -> Element {
    rsx! {
        div { class: "stat-card",
            h4 { "{title}" }
            div { class: "stat-value", "{value}" }
        }
    }
}
