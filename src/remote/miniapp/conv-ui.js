// OpenCrabs Mini App — conversation UI: task cards, conversation list,
// and the escape-first markdown subset. Loaded before app.js; shares its
// globals ($, tasks, sessionId, convsCache, callbacks) by design — one
// classic-script scope, no modules, no build step.

// ── render ─────────────────────────────────────────────────────────────
// Card structure is built with innerHTML, but every task-controlled
// string lands via textContent / renderMarkdown's escaped pipeline —
// no injection path from task content.
function render() {
  if (tasks.length === 0) {
    $("tasks").innerHTML =
      '<div class="card empty-welcome">' +
      '<h2 style="margin:0 0 8px;font-size:18px;">🦀 OpenCrabs Ready</h2>' +
      '<p class="dim" style="margin:0 0 12px;font-size:13px;">' +
      "Connected" +
      '</p>' +
      '<p class="hint" style="margin:0;font-size:12px;">Type a message below to drive your agent, or tap ☰ for past conversations.</p>' +
      '</div>';
    const sub = $("tasks").querySelector(".dim");
    if (sub)
      sub.textContent = currentUser ? "Connected as " + currentUser : "Connected to host";
    return;
  }
  $("tasks").innerHTML = tasks
    .map((t) => {
      const reply = t.reply ? '<div class="reply"></div>' : "";
      const you = t.you ? '<div class="you"></div>' : "";
      return (
        '<section class="task">' +
        '<div class="meta"><span class="state ' +
        t.state +
        '">' +
        t.state +
        "</span><span>" +
        (t.id ? short(t.id) : "…") +
        "</span></div>" +
        you +
        reply +
        "</section>"
      );
    })
    .join("");
  document.querySelectorAll(".task").forEach((el, i) => {
    const y = el.querySelector(".you");
    if (y) y.textContent = tasks[i].you;
    const r = el.querySelector(".reply");
    if (r) r.innerHTML = renderMarkdown(tasks[i].reply || "");
  });
  $("tasks").scrollTop = $("tasks").scrollHeight;
}

function renderConvs() {
  const el = $("conv-list");
  if (!el) return;
  if (!convsCache.length) {
    el.innerHTML =
      '<p class="hint">No conversations yet — tap ＋ New to start one.</p>';
    return;
  }
  el.innerHTML = convsCache
    .map(
      (c, i) =>
        '<div class="conv' +
        (c.id === sessionId ? " current" : "") +
        '" data-i="' +
        i +
        '">' +
        '<div class="conv-main"><div class="conv-title"></div>' +
        '<div class="conv-meta"></div></div>' +
        '<button class="ghost conv-act" data-act="rename" data-i="' +
        i +
        '">✎</button>' +
        '<button class="ghost conv-act" data-act="delete" data-i="' +
        i +
        '">🗑</button>' +
        "</div>"
    )
    .join("");
  el.querySelectorAll(".conv").forEach((row) => {
    const c = convsCache[Number(row.dataset.i)];
    row.querySelector(".conv-title").textContent = c.title || "(untitled)";
    row.querySelector(".conv-meta").textContent =
      (c.provider || "") +
      (c.model ? "/" + c.model : "") +
      " · " +
      (c.updatedAt || "").slice(0, 10);
    row.querySelector(".conv-main").addEventListener("click", () =>
      openConversation(c)
    );
  });
  el.querySelectorAll(".conv-act").forEach((btn) => {
    btn.addEventListener("click", () => {
      const c = convsCache[Number(btn.dataset.i)];
      if (btn.dataset.act === "rename") renameConversation(c);
      else deleteConversation(c);
    });
  });
}

// ── markdown (subset, escape-first) ───────────────────────────────────
// The TUI renders full markdown; the WebView gets the safe subset the
// agent actually emits: headings, bold/italic, code spans + fences,
// links (http/https only), lists, and tables. Raw HTML is escaped
// before any rule runs, so task content can never inject markup.
function esc(s) {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function inlineMd(s) {
  return s
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<b>$1</b>")
    .replace(/(^|[^*])\*([^*\n]+)\*/g, "$1<i>$2</i>")
    .replace(
      /\[([^\]]+)\]\((https?:[^)\s]+)\)/g,
      '<a href="$2" target="_blank" rel="noopener">$1</a>'
    );
}

function renderMarkdown(src) {
  const lines = esc(src).split("\n");
  const out = [];
  let inCode = false;
  let codeBuf = [];
  let inList = false;
  const closeList = () => {
    if (inList) {
      out.push("</ul>");
      inList = false;
    }
  };
  for (const line of lines) {
    if (line.trim().startsWith("```")) {
      if (inCode) {
        out.push("<pre><code>" + codeBuf.join("\n") + "</code></pre>");
        codeBuf = [];
        inCode = false;
      } else {
        closeList();
        inCode = true;
      }
      continue;
    }
    if (inCode) {
      codeBuf.push(line);
      continue;
    }
    const h = line.match(/^(#{1,4})\s+(.*)$/);
    if (h) {
      closeList();
      const lvl = Math.min(h[1].length + 2, 6);
      out.push("<h" + lvl + ">" + inlineMd(h[2]) + "</h" + lvl + ">");
      continue;
    }
    if (/^\s*[-*]\s+/.test(line)) {
      if (!inList) {
        out.push("<ul>");
        inList = true;
      }
      out.push("<li>" + inlineMd(line.replace(/^\s*[-*]\s+/, "")) + "</li>");
      continue;
    }
    closeList();
    if (line.trim() === "") {
      out.push("");
    } else if (/^\|.*\|$/.test(line.trim())) {
      out.push('<div class="md-table">' + inlineMd(line.trim()) + "</div>");
    } else {
      out.push("<p>" + inlineMd(line) + "</p>");
    }
  }
  if (inCode) out.push("<pre><code>" + codeBuf.join("\n") + "</code></pre>");
  closeList();
  return out.join("\n");
}

function short(id) {
  return id.length > 10 ? id.slice(0, 10) + "…" : id;
}

