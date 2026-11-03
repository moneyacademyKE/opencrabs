// OpenCrabs Mini App — slash commands: palette, dispatch, panels.
// Loaded after app.js (needs rpc/show/sessionId) and before the boot
// IIFE runs enter(). Shares the classic-script globals by design.
//
// Dispatch mirrors the TUI's handle_slash_command order:
// built-in local/panel → user command → skill (prompt) → unknown warning.
// The catalogue itself is server data (agent/commands) so the phone and
// the terminal can never drift.

// ── catalogue ─────────────────────────────────────────────────────────
async function loadCommands() {
  try {
    const body = await rpc("agent/commands", {});
    commandsCache = body.commands || [];
  } catch (e) {
    beacon("commands-failed", e.message);
  }
}

// ── palette ───────────────────────────────────────────────────────────
function hidePalette() {
  $("palette").hidden = true;
  $("palette").innerHTML = "";
}

function refreshPalette() {
  const text = $("composer").value;
  if (!text.startsWith("/") || text.includes(" ")) {
    hidePalette();
    return;
  }
  const needle = text.toLowerCase();
  const hits = commandsCache
    .filter((c) => c.name.toLowerCase().startsWith(needle))
    .slice(0, 8);
  if (!hits.length) {
    hidePalette();
    return;
  }
  $("palette").innerHTML = hits
    .map(
      (c, i) =>
        '<div class="pal-row" data-i="' +
        commandsCache.indexOf(c) +
        '"><span class="pal-name"></span><span class="pal-desc dim"></span></div>'
    )
    .join("");
  $("palette").querySelectorAll(".pal-row").forEach((row) => {
    const c = commandsCache[Number(row.dataset.i)];
    row.querySelector(".pal-name").textContent = c.name;
    row.querySelector(".pal-desc").textContent = c.description || "";
    row.addEventListener("click", () => {
      $("composer").value = c.name + " ";
      hidePalette();
      $("composer").focus();
      syncSendEnabled();
    });
  });
  $("palette").hidden = false;
}

// ── panels (Phase B: models + usage + help; Phase C adds the rest) ───
function openPanel(title) {
  panelReturn = "app";
  $("panel-title").textContent = title;
  $("panel-body").innerHTML = "";
  say("panel-err", "");
  show("panel");
}

function panelNote(text) {
  const p = document.createElement("p");
  p.className = "hint";
  p.textContent = text;
  $("panel-body").appendChild(p);
}

function panelCard(title, lines) {
  const div = document.createElement("div");
  div.className = "card panel-card";
  const h = document.createElement("h3");
  h.textContent = title;
  div.appendChild(h);
  for (const line of lines) {
    const p = document.createElement("p");
    p.className = "panel-line";
    p.textContent = line;
    div.appendChild(p);
  }
  $("panel-body").appendChild(div);
  return div;
}

async function openModelsPanel() {
  openPanel("Models");
  try {
    const body = await rpc("agent/models", {
      sessionId: sessionId || undefined,
    });
    const cur = body.current || {};
    panelNote("Current: " + (cur.provider || "?") + "/" + (cur.model || "?"));
    const picker = document.createElement("div");
    for (const provider of body.providers || []) {
      const btn = document.createElement("button");
      btn.className = "ghost model-provider";
      btn.textContent = provider.name || provider.id;
      btn.addEventListener("click", () =>
        renderModelChoices(provider, body.models || [])
      );
      picker.appendChild(btn);
    }
    $("panel-body").appendChild(picker);
    if (!(body.providers || []).length) {
      panelNote("No configured providers found on the host.");
    }
  } catch (e) {
    say("panel-err", e.message);
  }
}

function renderModelChoices(provider, models) {
  say("panel-err", "");
  const body = $("panel-body");
  body.querySelectorAll(".model-list").forEach((el) => el.remove());
  const list = document.createElement("div");
  list.className = "model-list";
  const h = document.createElement("p");
  h.className = "hint";
  h.textContent = (provider.name || provider.id) + " models:";
  list.appendChild(h);
  if (!models.length) {
    const p = document.createElement("p");
    p.className = "panel-line";
    p.textContent = "(provider returned no model list)";
    list.appendChild(p);
  }
  for (const model of models.slice(0, 40)) {
    const btn = document.createElement("button");
    btn.className = "ghost model-choice";
    btn.textContent = model;
    btn.addEventListener("click", () => setModel(provider.id, model));
    list.appendChild(btn);
  }
  body.appendChild(list);
}

async function setModel(providerId, model) {
  if (!sessionId) {
    say("panel-err", "Open or start a conversation first — model switches are per-session.");
    return;
  }
  try {
    const body = await rpc("agent/set-model", {
      sessionId,
      model: providerId + "/" + model,
    });
    if (!body.ok) throw new Error(body.reply || "switch failed");
    show("app");
    toast(body.reply || "Model switched.");
    health();
  } catch (e) {
    say("panel-err", e.message);
  }
}

async function openUsagePanel(period) {
  openPanel("Usage");
  try {
    const body = await rpc("agent/usage", { period: period || "today" });
    const s = body.summary || {};
    panelCard(body.period + " summary", [
      "Tokens: " + (s.totalTokens || 0),
      "Cost: $" + Number(s.totalCost || 0).toFixed(4),
      "Sessions: " + (s.sessionCount || 0) + " · Calls: " + (s.callCount || 0),
    ]);
    if ((body.models || []).length) {
      panelCard(
        "By model",
        body.models.map(
          (m) => m.model + " — " + m.tokens + " tok · $" + Number(m.cost).toFixed(4)
        )
      );
    }
    if ((body.tools || []).length) {
      panelCard(
        "Top tools",
        body.tools.map((t) => t.tool + " — " + t.calls + " calls")
      );
    }
    const periods = document.createElement("div");
    for (const p of ["today", "week", "month", "all"]) {
      const btn = document.createElement("button");
      btn.className = "ghost period-btn";
      btn.textContent = p;
      btn.addEventListener("click", () => openUsagePanel(p));
      periods.appendChild(btn);
    }
    $("panel-body").appendChild(periods);
  } catch (e) {
    say("panel-err", e.message);
  }
}

function openHelpPanel() {
  openPanel("Commands");
  const sections = ["BuiltIn", "Skill", "Custom"];
  for (const section of sections) {
    const rows = commandsCache.filter((c) => c.section === section);
    if (!rows.length) continue;
    panelCard(
      section === "BuiltIn" ? "Slash commands" : section + "s",
      rows.map((c) => c.name + " — " + (c.description || ""))
    );
  }
  if (!commandsCache.length)
    panelNote("Catalogue not loaded yet — check the gateway connection.");
}

// ── local command results ─────────────────────────────────────────────
// Local answers (clear/compact/plan/goal) render as their own card in
// the conversation, exactly where the TUI prints its system message.
function systemCard(text) {
  tasks.push({ id: null, state: "done", you: "", reply: text });
  render();
}

async function runLocal(method, params, fallbackText) {
  try {
    const body = await rpc(method, params);
    if (body.prompt) {
      // Server handed back a prompt for the one send path (compact /
      // plan-execute seed): send it as a visible turn.
      await sendText(body.prompt, fallbackText || method);
      if (body.reply) toast(body.reply);
      return true;
    }
    systemCard(body.reply || fallbackText || "Done.");
    return true;
  } catch (e) {
    say("app-err", e.message);
    return true; // handled (error surfaced), never falls through to a turn
  }
}

// ── dispatch ──────────────────────────────────────────────────────────
// Returns true when the input was a command (handled or refused), so
// send() never forwards it to the agent as chat text. Unknown commands
// get the TUI's warning, not an LLM turn.
async function maybeCommand(text) {
  const first = text.split(/\s+/)[0].toLowerCase();
  const args = text.slice(first.length).trim();
  const cmd = commandsCache.find((c) => c.name.toLowerCase() === first);

  // Session-scoped locals need a conversation; the chats view covers it.
  switch (first) {
    case "/new":
      await newConversation();
      return true;
    case "/sessions":
      await openConversations();
      return true;
    case "/stop":
      await stopStreaming();
      return true;
    case "/help":
      if (!commandsCache.length) await loadCommands();
      openHelpPanel();
      return true;
    case "/models":
      if (args) {
        // Direct switch, same as the TUI: /models provider/model
        if (!sessionId) {
          say("app-err", "Open or start a conversation first — model switches are per-session.");
          return true;
        }
        try {
          const body = await rpc("agent/set-model", { sessionId, model: args });
          if (!body.ok) throw new Error(body.reply || "switch failed");
          toast(body.reply);
        } catch (e) {
          say("app-err", e.message);
        }
        return true;
      }
      await openModelsPanel();
      return true;
    case "/usage":
      await openUsagePanel(args || "today");
      return true;
    case "/clear":
      if (!sessionId) {
        say("app-err", "No conversation to clear yet.");
        return true;
      }
      return runLocal("agent/clear-context", { sessionId });
    case "/compact":
      if (!sessionId) {
        say("app-err", "No conversation to compact yet.");
        return true;
      }
      return runLocal("agent/compact", { sessionId }, "/compact");
    case "/plan":
      if (!sessionId) {
        say("app-err", "Open or start a conversation first.");
        return true;
      }
      if (args) {
        // /plan <query>: arm plan mode, then run the query as the turn.
        await runLocal("agent/plan", { sessionId });
        await sendText(args);
        return true;
      }
      return runLocal("agent/plan", { sessionId });
    case "/show-plan":
    case "/showplan":
    case "/show_plan":
      if (!sessionId) {
        say("app-err", "No active conversation.");
        return true;
      }
      return runLocal("agent/plan-show", { sessionId });
    case "/execute":
      if (!sessionId) {
        say("app-err", "No active conversation.");
        return true;
      }
      return runLocal("agent/plan-execute", { sessionId }, "/execute");
    case "/discard":
      if (!sessionId) {
        say("app-err", "No active conversation.");
        return true;
      }
      return runLocal("agent/plan-discard", { sessionId });
    case "/rename": {
      if (!sessionId) {
        say("app-err", "No active conversation to rename.");
        return true;
      }
      try {
        await rpc("sessions/rename", { sessionId, title: args });
        setConversation(sessionId, contextId, args || convTitle);
        toast("Renamed.");
      } catch (e) {
        say("app-err", e.message);
      }
      return true;
    }
    case "/goal": {
      try {
        const body = await rpc("agent/goal-check", { args: " " + args });
        if (body.handled) {
          systemCard(body.reply);
          return true;
        }
        await sendText(body.prompt, text);
        return true;
      } catch (e) {
        say("app-err", e.message);
        return true;
      }
    }
  }

  // Panel commands whose browsers land in Phase C route through here so
  // the kind table and the dispatcher stay in agreement.
  if (cmd && cmd.kind === "panel") {
    toast(cmd.name + " browser lands with the Phase C dialogs.");
    return true;
  }

  // User commands: system-action prints, prompt-action sends its body.
  if (cmd && cmd.section === "Custom") {
    if (cmd.kind === "local-text") {
      systemCard(cmd.prompt || "");
      return true;
    }
    await sendText(cmd.prompt || text, text);
    return true;
  }

  // Skills: the slash dispatches the skill as a prompt turn (the gateway
  // resolves /<name> through the same skill loading the agent uses).
  if (cmd && cmd.kind === "prompt") {
    await sendText(text);
    return true;
  }

  say(
    "app-err",
    "⚡ Unknown command: " + first + ". Type /help for available commands."
  );
  return true;
}

// Send-button enablement: the old app silently no-oped on an empty
// composer (bk-0026); now the button visibly tracks content.
function syncSendEnabled() {
  $("send").disabled = !$("composer").value.trim();
}
