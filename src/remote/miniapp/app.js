// OpenCrabs Mini App — Phase 3 v1. Vanilla JS, same-origin, zero deps.
// Contract: POST /a2a/pair {code,label} → {token,scopes,expires_at};
// POST /a2a/v1 JSON-RPC message/send | tasks/get; GET /a2a/health.

const $ = (id) => document.getElementById(id);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const TERMINAL = ["completed", "failed", "canceled", "rejected"];

// Resilient SafeStorage: in sandboxed WebViews (Telegram Desktop, iOS WebKit),
// raw localStorage can throw SecurityError or QuotaExceededError. We catch
// exceptions and transparently fall back to an in-memory dictionary.
const memStorage = {};
const Storage = {
  get(key) {
    try {
      const val = localStorage.getItem(key);
      if (val !== null) return val;
    } catch {}
    return memStorage[key] || null;
  },
  set(key, val) {
    memStorage[key] = String(val);
    try {
      localStorage.setItem(key, val);
    } catch {}
  },
  remove(key) {
    delete memStorage[key];
    try {
      localStorage.removeItem(key);
    } catch {}
  },
};

let token = Storage.get("oc_token");
let scopes = JSON.parse(Storage.get("oc_scopes") || "[]");
let currentUser = Storage.get("oc_user");
const tasks = [];
// ── conversations (Phase A parity) ─────────────────────────────────
// The composer acts on ONE conversation, identified three ways that must
// agree: sessionId (the conversation row), contextId (the A2A continuity
// key bound to it), and taskId (the turn currently in flight, for stop).
// Persisted so a WebView reload resumes where the user left off instead
// of the old memory-only loss (bk-0026).
let sessionId = Storage.get("oc_session");
let contextId = Storage.get("oc_context");
let convTitle = Storage.get("oc_conv_title");
let activeTaskId = null;
let activeAbort = null;
let convsCache = [];

// ── diagnostic beacon (WebView consoles are invisible) ─────────────
function beacon(step, detail) {
  try {
    const q = "s=" + encodeURIComponent(step) +
              "&d=" + encodeURIComponent(detail || "");
    fetch("/a2a/app/beacon?" + q).catch(() => {});
  } catch {}
}

// Global error trap: any unhandled exception becomes a beacon.
window.onerror = function(msg, src, line, col) {
  beacon("js-error", msg + " at " + (src||"?") + ":" + line + ":" + col);
};
window.addEventListener("unhandledrejection", function(ev) {
  beacon("promise-error", String(ev.reason));
});

beacon("script-loaded", "token=" + (token ? "yes" : "no"));

// ── views ──────────────────────────────────────────────────────────────
function show(view) {
  $("login").hidden = view !== "login";
  $("app").hidden = view !== "app";
  $("convs").hidden = view !== "convs";
}
function say(id, msg) {
  const el = $(id);
  el.textContent = msg || "";
  el.hidden = !msg;
}
// Transient confirmation line (post-pair success, saved rename, …).
// The old app succeeded silently and silence read as failure (bk-0026).
let toastTimer = null;
function toast(msg) {
  const el = $("toast");
  el.textContent = msg || "";
  el.hidden = !msg;
  clearTimeout(toastTimer);
  if (msg) toastTimer = setTimeout(() => { el.hidden = true; }, 4000);
}

// ── auth ───────────────────────────────────────────────────────────────
async function pair() {
  const code = $("code").value.trim().toUpperCase();
  if (code.length < 4) {
    say("login-err", "Enter the code the host minted with `opencrabs connect`.");
    return;
  }
  $("pair-btn").disabled = true;
  say("login-err", "");
  try {
    const label = $("label").value.trim() || "miniapp";
    const res = await fetch("/a2a/pair", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ code, label }),
    });
    if (res.status === 401) throw new Error("Invalid or expired code.");
    if (res.status === 429)
      throw new Error("Too many tries — locked out. Try again in a few minutes.");
    if (!res.ok) throw new Error("Pair failed (HTTP " + res.status + ").");
    const body = await res.json();
    token = body.token;
    scopes = body.scopes || [];
    currentUser = label;
    Storage.set("oc_token", token);
    Storage.set("oc_scopes", JSON.stringify(scopes));
    Storage.set("oc_user", currentUser);
    enter();
  } catch (e) {
    say("login-err", e.message);
  } finally {
    $("pair-btn").disabled = false;
  }
}

function logout() {
  token = null;
  scopes = [];
  currentUser = null;
  Storage.remove("oc_token");
  Storage.remove("oc_scopes");
  Storage.remove("oc_user");
  setConversation(null, null, null);
  tasks.length = 0;
  $("tasks").innerHTML = "";
  if ($("auth-label")) $("auth-label").textContent = "";
  show("login");
}

function setConversation(sid, ctx, title) {
  sessionId = sid;
  contextId = ctx;
  convTitle = title;
  if (sid) Storage.set("oc_session", sid); else Storage.remove("oc_session");
  if (ctx) Storage.set("oc_context", ctx); else Storage.remove("oc_context");
  if (title) Storage.set("oc_conv_title", title);
  else Storage.remove("oc_conv_title");
  if ($("conv-label")) $("conv-label").textContent = title ? "· " + title : "";
}

function enter() {
  beacon("enter", "calling show(app)");
  show("app");
  if ($("auth-label")) {
    $("auth-label").textContent = currentUser ? "· " + currentUser : "";
  }
  if ($("conv-label") && convTitle) {
    $("conv-label").textContent = "· " + convTitle;
  }
  render();
  health();
  restoreConversation();
  beacon("enter-done", "app visible=" + !$("app").hidden);
}

// ── gateway calls ──────────────────────────────────────────────────────
async function rpc(method, params) {
  const res = await fetch("/a2a/v1", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: "Bearer " + token,
    },
    body: JSON.stringify({ jsonrpc: "2.0", id: Date.now(), method, params }),
  });
  if (res.status === 401) {
    logout();
    throw new Error("Session expired — pair again.");
  }
  if (!res.ok) throw new Error("Gateway HTTP " + res.status + ".");
  const body = await res.json();
  if (body.error)
    throw new Error(body.error.message + " (code " + body.error.code + ")");
  return body.result;
}

async function health() {
  try {
    const res = await fetch("/a2a/health");
    const body = await res.json();
    $("dot").className = "dot " + (body.status === "ok" ? "ok" : "bad");
    $("host-label").textContent = "OpenCrabs " + (body.version || "");
  } catch {
    $("dot").className = "dot bad";
  }
}

// ── conversations ─────────────────────────────────────────────────────
async function loadConversations() {
  const body = await rpc("sessions/list", { limit: 50 });
  convsCache = body.sessions || [];
  renderConvs();
}

async function openConversations() {
  show("convs");
  say("conv-err", "");
  try {
    await loadConversations();
  } catch (e) {
    say("conv-err", e.message);
  }
}

async function newConversation() {
  say("app-err", "");
  try {
    const s = await rpc("sessions/create", {});
    setConversation(s.id, s.contextId, s.title || "New conversation");
    tasks.length = 0;
    render();
    show("app");
    toast("New conversation — type below to drive your agent.");
    beacon("conv-new", s.id);
  } catch (e) {
    say("app-err", e.message);
  }
}

async function openConversation(c) {
  say("conv-err", "");
  try {
    const body = await rpc("sessions/history", { sessionId: c.id, limit: 100 });
    // Server binds the send-context on create; for a conversation that
    // didn't come from the gateway we resume under its own id — the
    // context→session lookup creates the binding on first send (#1159).
    setConversation(c.id, c.id, c.title || "Conversation");
    tasks.length = 0;
    for (const m of body.messages || []) {
      if (m.role === "user") {
        tasks.push({ id: m.id, state: "done", you: m.content, reply: "" });
      } else if (tasks.length && !tasks[tasks.length - 1].reply) {
        tasks[tasks.length - 1].reply = m.content;
      } else {
        tasks.push({ id: m.id, state: "done", you: "", reply: m.content });
      }
    }
    show("app");
    render();
    beacon("conv-open", c.id + " msgs=" + tasks.length);
  } catch (e) {
    say("conv-err", e.message);
  }
}

async function renameConversation(c) {
  const title = prompt("Rename conversation", c.title || "");
  if (title === null) return;
  try {
    await rpc("sessions/rename", { sessionId: c.id, title });
    if (sessionId === c.id) setConversation(c.id, contextId, title || convTitle);
    await loadConversations();
    toast("Renamed.");
  } catch (e) {
    say("conv-err", e.message);
  }
}

async function deleteConversation(c) {
  if (!confirm("Archive this conversation? (restorable from the host TUI)")) return;
  try {
    await rpc("sessions/delete", { sessionId: c.id });
    if (sessionId === c.id) setConversation(null, null, null);
    await loadConversations();
    toast("Archived.");
  } catch (e) {
    say("conv-err", e.message);
  }
}

// After a reload with a persisted conversation, repaint its history so
// the screen never opens blank over a live thread.
async function restoreConversation() {
  if (!sessionId || tasks.length) return;
  const mine = sessionId;
  try {
    const body = await rpc("sessions/history", { sessionId: mine, limit: 100 });
    if (sessionId !== mine) return; // user switched meanwhile
    for (const m of body.messages || []) {
      if (m.role === "user") {
        tasks.push({ id: m.id, state: "done", you: m.content, reply: "" });
      } else if (tasks.length && !tasks[tasks.length - 1].reply) {
        tasks[tasks.length - 1].reply = m.content;
      } else {
        tasks.push({ id: m.id, state: "done", you: "", reply: m.content });
      }
    }
    render();
    beacon("conv-restored", mine + " msgs=" + tasks.length);
  } catch (e) {
    beacon("conv-restore-failed", e.message);
  }
}

// ── telegram initData auto-login ───────────────────────────────────────
// Inside Telegram's webview, window.Telegram.WebApp.initData is a
// launch payload signed with the bot's own token. The gateway verifies
// that signature (POST /a2a/telegram/init-data) and, if the user is on
// [a2a].telegram_users, hands back a session token — no code typing.
async function tryInitData() {
  beacon("initdata-start", "checking window.Telegram");
  const tg = window.Telegram && window.Telegram.WebApp;
  if (!tg) {
    beacon("initdata-no-sdk", "window.Telegram=" + typeof window.Telegram);
    return false;
  }
  try {
    if (tg.ready) tg.ready();
    if (tg.expand) tg.expand();
  } catch {}

  const initData = tg.initData;
  beacon("initdata-check", "len=" + (initData ? initData.length : 0));
  if (!initData) {
    say(
      "login-err",
      "Telegram session not detected. Please open OpenCrabs via the bot's Menu Button (bottom-left) to auto-login."
    );
    beacon("initdata-empty", "no initData");
    return false;
  }

  try {
    beacon("initdata-fetch", "posting to /a2a/telegram/init-data");
    const res = await fetch("/a2a/telegram/init-data", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ init_data: initData, label: "miniapp" }),
    });
    beacon("initdata-response", "status=" + res.status);
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      const err = body.error || ("HTTP " + res.status);
      say("login-err", "Telegram auto-login failed: " + err);
      beacon("initdata-rejected", err);
      return false;
    }
    const body = await res.json();
    beacon("initdata-body", "token=" + (body.token ? "yes" : "no") +
           " user=" + (body.username || body.user_id || "?"));
    token = body.token;
    scopes = body.scopes || [];
    currentUser = body.username
      ? "@" + body.username
      : (body.user_id ? "ID " + body.user_id : null);
    Storage.set("oc_token", token);
    Storage.set("oc_scopes", JSON.stringify(scopes));
    if (currentUser) Storage.set("oc_user", currentUser);
    say("login-err", "");
    beacon("initdata-ok", "returning true");
    return true;
  } catch (e) {
    say("login-err", "Network error during auto-login: " + e.message);
    beacon("initdata-error", e.message);
    return false;
  }
}

// ── wire up ────────────────────────────────────────────────────────────
$("pair-btn").addEventListener("click", pair);
$("code").addEventListener("keydown", (e) => {
  if (e.key === "Enter") pair();
});
$("logout").addEventListener("click", logout);
$("send").addEventListener("click", send);
$("stop").addEventListener("click", stopStreaming);
$("convs-btn").addEventListener("click", openConversations);
$("new-conv").addEventListener("click", newConversation);
$("back-chat").addEventListener("click", () => show("app"));
$("composer").addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    send();
  }
});
(async () => {
  beacon("boot", "token=" + (token ? "yes" : "no"));
  try {
    if (await tryInitData()) {
      beacon("boot-initdata-ok", "calling enter");
      enter();
    } else if (token) {
      beacon("boot-stale-token", "calling enter");
      enter();
    } else {
      beacon("boot-no-auth", "showing login");
      show("login");
    }
    beacon("boot-done", "login.hidden=" + $("login").hidden + " app.hidden=" + $("app").hidden);
  } catch (e) {
    beacon("boot-crash", e.message + " | " + e.stack);
    show("login");
  }
})();
