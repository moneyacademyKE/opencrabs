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

// ── views ──────────────────────────────────────────────────────────────
function show(view) {
  $("login").hidden = view !== "login";
  $("app").hidden = view !== "app";
}
function say(id, msg) {
  const el = $(id);
  el.textContent = msg || "";
  el.hidden = !msg;
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
  tasks.length = 0;
  $("tasks").innerHTML = "";
  if ($("auth-label")) $("auth-label").textContent = "";
  show("login");
}

function enter() {
  show("app");
  if ($("auth-label")) {
    $("auth-label").textContent = currentUser ? "· " + currentUser : "";
  }
  render();
  health();
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

// ── tasks ──────────────────────────────────────────────────────────────
async function send() {
  const text = $("composer").value.trim();
  if (!text || !token) return;
  $("send").disabled = true;
  say("app-err", "");
  try {
    const task = await rpc("message/send", {
      message: { role: "user", parts: [{ text }] },
    });
    if (scopes.length && !scopes.includes("a2a:send"))
      say("app-err", "This session is read-only (no a2a:send scope).");
    tasks.unshift({
      id: task.id,
      state: (task.status && task.status.state) || "working",
      you: text,
      reply: replyText(task),
    });
    $("composer").value = "";
    render();
    poll(task.id);
  } catch (e) {
    say("app-err", e.message);
  } finally {
    $("send").disabled = false;
  }
}

async function poll(id) {
  for (;;) {
    await sleep(1200);
    let task;
    try {
      task = await rpc("tasks/get", { id });
    } catch (e) {
      const t = tasks.find((x) => x.id === id);
      if (t) {
        t.state = "failed";
        t.reply = e.message;
        render();
      }
      return;
    }
    const state = (task.status && task.status.state) || "working";
    const t = tasks.find((x) => x.id === id);
    if (t) {
      t.state = state;
      t.reply = replyText(task) || t.reply;
      render();
    }
    if (TERMINAL.includes(state)) return;
  }
}

// The agent's answer: artifacts first, then the status message.
function replyText(task) {
  const out = [];
  for (const a of task.artifacts || [])
    for (const p of a.parts || []) if (p.text) out.push(p.text);
  const msg = task.status && task.status.message;
  if (out.length === 0 && msg)
    for (const p of msg.parts || []) if (p.text) out.push(p.text);
  return out.join("\n");
}

// ── render ─────────────────────────────────────────────────────────────
// innerHTML only builds structure; all task-controlled text goes in via
// textContent afterwards — no injection path from task content.
function render() {
  if (tasks.length === 0) {
    $("tasks").innerHTML =
      '<div class="card empty-welcome">' +
      '<h2 style="margin:0 0 8px;font-size:18px;">🦀 OpenCrabs Ready</h2>' +
      '<p class="dim" style="margin:0 0 12px;font-size:13px;">' +
      (currentUser ? 'Connected as ' + currentUser : 'Connected to host') +
      '</p>' +
      '<p class="hint" style="margin:0;font-size:12px;">Type a message below to drive your agent.</p>' +
      '</div>';
    return;
  }
  $("tasks").innerHTML = tasks
    .map((t) => {
      const reply = t.reply ? '<div class="reply"></div>' : "";
      return (
        '<section class="task">' +
        '<div class="meta"><span class="state ' +
        t.state +
        '">' +
        t.state +
        "</span><span>" +
        short(t.id) +
        "</span></div>" +
        '<div class="you"></div>' +
        reply +
        "</section>"
      );
    })
    .join("");
  document.querySelectorAll(".task").forEach((el, i) => {
    el.querySelector(".you").textContent = tasks[i].you;
    const r = el.querySelector(".reply");
    if (r) r.textContent = tasks[i].reply || "";
  });
  $("tasks").scrollTop = 0;
}

function short(id) {
  return id.length > 10 ? id.slice(0, 10) + "…" : id;
}

// ── telegram initData auto-login ───────────────────────────────────────
// Inside Telegram's webview, window.Telegram.WebApp.initData is a
// launch payload signed with the bot's own token. The gateway verifies
// that signature (POST /a2a/telegram/init-data) and, if the user is on
// [a2a].telegram_users, hands back a session token — no code typing.
async function tryInitData() {
  const tg = window.Telegram && window.Telegram.WebApp;
  if (!tg) return false;
  try {
    if (tg.ready) tg.ready();
    if (tg.expand) tg.expand();
  } catch {}

  const initData = tg.initData;
  if (!initData) {
    say(
      "login-err",
      "Telegram session not detected. Please open OpenCrabs via the bot's Menu Button (bottom-left) to auto-login."
    );
    return false;
  }

  try {
    const res = await fetch("/a2a/telegram/init-data", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ init_data: initData, label: "miniapp" }),
    });
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      const err = body.error || ("HTTP " + res.status);
      say("login-err", "Telegram auto-login failed: " + err);
      return false;
    }
    const body = await res.json();
    token = body.token;
    scopes = body.scopes || [];
    currentUser = body.username
      ? "@" + body.username
      : (body.user_id ? "ID " + body.user_id : null);
    Storage.set("oc_token", token);
    Storage.set("oc_scopes", JSON.stringify(scopes));
    if (currentUser) Storage.set("oc_user", currentUser);
    say("login-err", "");
    return true;
  } catch (e) {
    say("login-err", "Network error during auto-login: " + e.message);
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
$("composer").addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    send();
  }
});
(async () => {
  if (await tryInitData()) {
    enter();
  } else if (token) {
    enter();
  } else {
    show("login");
  }
})();
