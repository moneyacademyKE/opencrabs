// ── tasks ──────────────────────────────────────────────────────────────
// OpenCrabs Mini App — turn engine: SSE streaming send, poll fallback,
// stop/cancel. Loaded between conv-ui.js and app.js; shares their
// classic-script globals by design (no modules, no build step).

// ── tasks ──────────────────────────────────────────────────────────────
// Sending rides `message/stream` (SSE): tokens arrive as events instead
// of the old 1.2s poll. `message/send` + `tasks/get` polling remains as
// the fallback for WebViews where streaming fetch misbehaves.
async function send() {
  const text = $("composer").value.trim();
  if (!text) {
    say("app-err", "Type a message first — the composer is empty.");
    return;
  }
  if (!token) {
    say("app-err", "Not paired — reopen via the bot's menu button.");
    return;
  }
  $("send").disabled = true;
  say("app-err", "");
  const card = { id: null, state: "working", you: text, reply: "" };
  tasks.push(card);
  $("composer").value = "";
  render();
  try {
    if (!sessionId) await newConversationSilent();
    await streamMessage(card, text);
  } catch (e) {
    // Streaming failed mid-flight: fall back to send+poll once.
    try {
      await pollFallback(card, text);
    } catch (e2) {
      card.state = "failed";
      card.reply = e2.message;
      render();
    }
  } finally {
    $("send").disabled = false;
    activeTaskId = null;
    setStreaming(false);
  }
}

async function newConversationSilent() {
  const s = await rpc("sessions/create", {});
  setConversation(s.id, s.contextId, s.title || "");
}

async function streamMessage(card, text) {
  const ctrl = new AbortController();
  activeAbort = ctrl;
  const res = await fetch("/a2a/v1", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: "Bearer " + token,
    },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: Date.now(),
      method: "message/stream",
      params: {
        message: {
          role: "user",
          parts: [{ text }],
          contextId: contextId || undefined,
        },
      },
    }),
    signal: ctrl.signal,
  });
  if (res.status === 401) {
    logout();
    throw new Error("Session expired — pair again.");
  }
  if (!res.ok || !res.body) throw new Error("Gateway HTTP " + res.status + ".");
  setStreaming(true);
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buf = "";
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true });
    let cut;
    while ((cut = buf.indexOf("\n\n")) >= 0) {
      const frame = buf.slice(0, cut);
      buf = buf.slice(cut + 2);
      const line = frame.split("\n").find((l) => l.startsWith("data:"));
      if (!line) continue;
      let ev;
      try {
        ev = JSON.parse(line.slice(5).trim());
      } catch {
        continue;
      }
      applyStreamEvent(card, ev.result || ev);
    }
  }
  if (!TERMINAL.includes((card.state || "").toLowerCase())) {
    card.state = "completed";
    render();
  }
}

function applyStreamEvent(card, ev) {
  if (!ev || typeof ev !== "object") return;
  if (ev.id && !card.id) {
    card.id = ev.id;
    activeTaskId = ev.id;
  }
  if (ev.contextId && ev.contextId !== contextId) {
    contextId = ev.contextId;
    Storage.set("oc_context", contextId);
  }
  const kind = ev.kind || "";
  if (kind === "artifact-update") {
    const parts = (ev.artifact && ev.artifact.parts) || [];
    const text = parts.map((p) => p.text || "").join("");
    card.reply = ev.append ? (card.reply || "") + text : text || card.reply;
  } else if (kind === "status-update") {
    const state = ev.status && ev.status.state;
    if (state) card.state = state.toLowerCase();
  } else if (ev.status && ev.status.state) {
    card.state = ev.status.state.toLowerCase();
    const reply = replyText(ev);
    if (reply) card.reply = reply;
  }
  render();
}

// Stop button: cancel the in-flight task server-side, then cut the
// stream. The cancel token kills the agent turn, not just the display.
async function stopStreaming() {
  if (activeTaskId) {
    try {
      await rpc("tasks/cancel", { id: activeTaskId });
    } catch {}
  }
  if (activeAbort) activeAbort.abort();
  setStreaming(false);
}

function setStreaming(on) {
  $("stop").hidden = !on;
  $("send").hidden = on;
}

async function pollFallback(card, text) {
  const task = await rpc("message/send", {
    message: { role: "user", parts: [{ text }], contextId: contextId || undefined },
  });
  card.id = task.id;
  activeTaskId = task.id;
  if (scopes.length && !scopes.includes("a2a:send"))
    say("app-err", "This session is read-only (no a2a:send scope).");
  render();
  await pollInto(card);
}

async function pollInto(card) {
  for (;;) {
    await sleep(1200);
    const task = await rpc("tasks/get", { id: card.id });
    const state = ((task.status && task.status.state) || "working").toLowerCase();
    card.state = state;
    card.reply = replyText(task) || card.reply;
    render();
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

