// OpenCrabs Mini App — Phase 3 v1. Vanilla JS, same-origin, zero deps.
// Contract: POST /a2a/pair {code,label} → {token,scopes,expires_at};
// POST /a2a/v1 JSON-RPC message/send | tasks/get; GET /a2a/health.

const $ = (id) => document.getElementById(id);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const TERMINAL = ["completed", "failed", "canceled", "rejected"];

let token = sessionStorage.getItem("oc_token");
let scopes = JSON.parse(sessionStorage.getItem("oc_scopes") || "[]");
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
    const res = await fetch("/a2a/pair", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        code,
        label: $("label").value.trim() || "miniapp",
      }),
    });
    if (res.status === 401) throw new Error("Invalid or expired code.");
    if (res.status === 429)
      throw new Error("Too many tries — locked out. Try again in a few minutes.");
    if (!res.ok) throw new Error("Pair failed (HTTP " + res.status + ").");
    const body = await res.json();
    token = body.token;
    scopes = body.scopes || [];
    sessionStorage.setItem("oc_token", token);
    sessionStorage.setItem("oc_scopes", JSON.stringify(scopes));
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
  sessionStorage.removeItem("oc_token");
  sessionStorage.removeItem("oc_scopes");
  tasks.length = 0;
  $("tasks").innerHTML = "";
  show("login");
}

function enter() {
  show("app");
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
if (token) enter();
else show("login");
