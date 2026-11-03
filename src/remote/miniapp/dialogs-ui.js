// OpenCrabs Mini App — Phase C dialogs: skills browser, profiles,
// mission-control report, web themes. Loaded after app.js/cmd-panels.js;
// shares their classic-script globals by design.

// ── Phase C dialog panels ────────────────────────────────────────────
async function openSkillsPanel(filter) {
  openPanel("Skills");
  try {
    const body = await rpc("agent/skills", {});
    const all = body.skills || [];
    const needle = (filter || "").toLowerCase();
    const hits = needle
      ? all.filter(
          (s) =>
            s.slash.toLowerCase().includes(needle) ||
            (s.description || "").toLowerCase().includes(needle)
        )
      : all;
    panelNote(
      hits.length + " of " + all.length + " skills" +
        (needle ? " matching “" + filter + "”" : "") +
        " — tap one to run it."
    );
    const list = document.createElement("div");
    for (const s of hits.slice(0, 60)) {
      const btn = document.createElement("button");
      btn.className = "ghost skill-row";
      btn.textContent = s.slash + (s.reviewGate ? " 🔒" : "");
      btn.title = s.description || "";
      btn.addEventListener("click", () => runSkill(s));
      list.appendChild(btn);
      const desc = document.createElement("p");
      desc.className = "panel-line dim";
      desc.textContent = s.description || "";
      list.appendChild(desc);
    }
    if (!hits.length) panelNote("No skills match.");
    $("panel-body").appendChild(list);
  } catch (e) {
    say("panel-err", e.message);
  }
}

async function runSkill(s) {
  if (s.reviewGate) {
    const ok = confirm(
      s.slash + " declares a review gate: its output must be presented " +
        "for approval before anything ships. Run it?"
    );
    if (!ok) return;
  }
  show("app");
  await sendText(s.slash);
}

async function openProfilesPanel() {
  openPanel("Profiles");
  try {
    const body = await rpc("agent/profiles", {});
    const rows = body.profiles || [];
    panelCard(
      "Profiles (read-only)",
      rows.map(
        (p) =>
          (p.active ? "● " : "○ ") +
          p.name +
          (p.description ? " — " + p.description : "") +
          (p.lastUsed ? " · last used " + String(p.lastUsed).slice(0, 10) : "")
      )
    );
    panelNote(
      "Creating, switching, or migrating a profile changes which " +
        "config the host runs under — do that at the host (TUI /profiles)."
    );
  } catch (e) {
    say("panel-err", e.message);
  }
}

async function openMissionPanel() {
  openPanel("Mission Control");
  try {
    const body = await rpc("agent/mission-control", {});
    const card = panelCard("Report", []);
    const pre = document.createElement("div");
    pre.className = "reply";
    pre.innerHTML = renderMarkdown(body.report || "");
    card.appendChild(pre);
  } catch (e) {
    say("panel-err", e.message);
  }
}

// ── web themes (client-side; TUI terminal themes don't translate) ────
const THEMES = {
  "crab-dark": {}, // the shipped :root values
  "midnight": {
    "--bg": "#0a0e1a", "--panel": "#121828", "--edge": "#26314d",
    "--text": "#dfe7ff", "--dim": "#7d8db0", "--accent": "#6ea8ff",
  },
  "forest": {
    "--bg": "#0c1410", "--panel": "#14201a", "--edge": "#2a3d33",
    "--text": "#e2f2e7", "--dim": "#83a08e", "--accent": "#5fd68b",
  },
  "paper": {
    "--bg": "#f6f3ec", "--panel": "#fffdf6", "--edge": "#d8d2c2",
    "--text": "#24211a", "--dim": "#7a7260", "--accent": "#d2542e",
  },
};

function applyTheme(name) {
  const vars = THEMES[name];
  if (!vars) return false;
  const root = document.documentElement.style;
  for (const key of ["--bg", "--panel", "--edge", "--text", "--dim", "--accent"])
    root.removeProperty(key);
  for (const [key, val] of Object.entries(vars)) root.setProperty(key, val);
  Storage.set("oc_theme", name);
  return true;
}

function openThemePanel() {
  openPanel("Theme");
  panelNote("Web themes for this app (the TUI's terminal themes stay in the terminal).");
  for (const name of Object.keys(THEMES)) {
    const btn = document.createElement("button");
    btn.className = "ghost theme-row";
    btn.textContent =
      (Storage.get("oc_theme") === name || (!Storage.get("oc_theme") && name === "crab-dark")
        ? "● "
        : "○ ") + name;
    btn.addEventListener("click", () => {
      applyTheme(name);
      openThemePanel();
      toast("Theme: " + name);
    });
    $("panel-body").appendChild(btn);
  }
}

function handleThemeCommand(args) {
  const parts = args.split(/\s+/).filter(Boolean);
  const sub = parts[0] || "";
  const name = parts[1] || "";
  if (sub === "" ) {
    openThemePanel();
    return true;
  }
  if (sub === "list" || sub === "ls") {
    systemCard(
      "Web themes: " +
        Object.keys(THEMES).join(", ") +
        "\nActive: " +
        (Storage.get("oc_theme") || "crab-dark") +
        "\nUse: /theme set <name> · /theme reset"
    );
    return true;
  }
  if (sub === "set") {
    if (!name) systemCard("Usage: /theme set <name>");
    else if (applyTheme(name)) systemCard("Theme switched to '" + name + "'.");
    else
      systemCard(
        "Unknown theme '" + name + "'. Web themes: " + Object.keys(THEMES).join(", ")
      );
    return true;
  }
  if (sub === "reset") {
    applyTheme("crab-dark");
    systemCard("Theme reset to crab-dark (default).");
    return true;
  }
  systemCard("Unknown /theme subcommand '" + sub + "'. Try: list | set <name> | reset");
  return true;
}

