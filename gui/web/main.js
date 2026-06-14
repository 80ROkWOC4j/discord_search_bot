const invoke = window.__TAURI__?.core?.invoke;
const CONFIG_STORAGE_KEY = "discord-search-bot:launcher-config";
const DEFAULT_CONFIG = {
  token: "",
  client_id: ""
};

const fields = {
  status: document.querySelector("#status"),
  token: document.querySelector("#token"),
  logOutput: document.querySelector("#log-output"),
  toggleBot: document.querySelector("#toggle-bot")
};

let botRunning = false;
let launcherConfig = normalizeConfig();

function setStatus(text, kind = "idle") {
  fields.status.textContent = text;
  fields.status.dataset.kind = kind;
}

function getConfig() {
  return {
    token: fields.token.value.trim(),
    client_id: launcherConfig.client_id
  };
}

function normalizeConfig(config = {}) {
  return {
    token: String(config.token || DEFAULT_CONFIG.token),
    client_id: String(config.client_id || DEFAULT_CONFIG.client_id)
  };
}

function applyConfig(config) {
  const normalized = normalizeConfig(config);
  launcherConfig = normalized;
  fields.token.value = normalized.token;
}

function loadStoredConfig() {
  try {
    return normalizeConfig(JSON.parse(localStorage.getItem(CONFIG_STORAGE_KEY) || "{}"));
  } catch {
    return normalizeConfig();
  }
}

function saveStoredConfig(config) {
  const stored = normalizeConfig(config);
  launcherConfig = stored;
  localStorage.setItem(CONFIG_STORAGE_KEY, JSON.stringify(stored));
}

function clearStoredConfig() {
  localStorage.removeItem(CONFIG_STORAGE_KEY);
}

const ANSI_FG_COLORS = {
  30: "#7d8590",
  31: "#ff7b72",
  32: "#7ee787",
  33: "#d29922",
  34: "#79c0ff",
  35: "#d2a8ff",
  36: "#76e3ea",
  37: "#d8dee9",
  90: "#8b949e",
  91: "#ffa198",
  92: "#56d364",
  93: "#e3b341",
  94: "#a5d6ff",
  95: "#d2a8ff",
  96: "#b3f0ff",
  97: "#ffffff"
};

const ANSI_BG_COLORS = {
  40: "#30363d",
  41: "#67060c",
  42: "#033a16",
  43: "#4b2900",
  44: "#0d419d",
  45: "#512a97",
  46: "#05595b",
  47: "#d8dee9",
  100: "#484f58",
  101: "#da3633",
  102: "#238636",
  103: "#9e6a03",
  104: "#388bfd",
  105: "#8957e5",
  106: "#2b7489",
  107: "#ffffff"
};

function escapeHtml(text) {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function xtermColor(value) {
  if (value < 0 || value > 255) {
    return null;
  }
  if (value < 16) {
    const basic = [
      30, 31, 32, 33, 34, 35, 36, 37,
      90, 91, 92, 93, 94, 95, 96, 97
    ];
    return ANSI_FG_COLORS[basic[value]];
  }
  if (value >= 232) {
    const level = 8 + (value - 232) * 10;
    return `rgb(${level}, ${level}, ${level})`;
  }

  const index = value - 16;
  const levels = [0, 95, 135, 175, 215, 255];
  const red = levels[Math.floor(index / 36)];
  const green = levels[Math.floor(index / 6) % 6];
  const blue = levels[index % 6];
  return `rgb(${red}, ${green}, ${blue})`;
}

function resetAnsiState(state) {
  state.fg = null;
  state.bg = null;
  state.bold = false;
  state.dim = false;
  state.italic = false;
  state.underline = false;
}

function applyAnsiCodes(rawCodes, state) {
  const codes = rawCodes === "" ? [0] : rawCodes.split(";").map((code) => Number(code || 0));

  for (let index = 0; index < codes.length; index += 1) {
    const code = codes[index];

    if (code === 0) resetAnsiState(state);
    else if (code === 1) state.bold = true;
    else if (code === 2) state.dim = true;
    else if (code === 3) state.italic = true;
    else if (code === 4) state.underline = true;
    else if (code === 22) {
      state.bold = false;
      state.dim = false;
    } else if (code === 23) state.italic = false;
    else if (code === 24) state.underline = false;
    else if (code === 39) state.fg = null;
    else if (code === 49) state.bg = null;
    else if (ANSI_FG_COLORS[code]) state.fg = ANSI_FG_COLORS[code];
    else if (ANSI_BG_COLORS[code]) state.bg = ANSI_BG_COLORS[code];
    else if ((code === 38 || code === 48) && codes[index + 1] === 2) {
      const red = codes[index + 2];
      const green = codes[index + 3];
      const blue = codes[index + 4];
      const color = `rgb(${red}, ${green}, ${blue})`;
      if (code === 38) state.fg = color;
      else state.bg = color;
      index += 4;
    } else if ((code === 38 || code === 48) && codes[index + 1] === 5) {
      const color = xtermColor(codes[index + 2]);
      if (color && code === 38) state.fg = color;
      else if (color) state.bg = color;
      index += 2;
    }
  }
}

function wrapAnsiText(text, state) {
  if (!text) {
    return "";
  }

  const classes = [];
  const styles = [];
  if (state.bold) classes.push("ansi-bold");
  if (state.dim) classes.push("ansi-dim");
  if (state.italic) classes.push("ansi-italic");
  if (state.underline) classes.push("ansi-underline");
  if (state.fg) styles.push(`color: ${state.fg}`);
  if (state.bg) styles.push(`background-color: ${state.bg}`);

  const escaped = escapeHtml(text);
  if (!classes.length && !styles.length) {
    return escaped;
  }

  const classAttr = classes.length ? ` class="${classes.join(" ")}"` : "";
  const styleAttr = styles.length ? ` style="${styles.join("; ")}"` : "";
  return `<span${classAttr}${styleAttr}>${escaped}</span>`;
}

function renderAnsi(line) {
  const state = {};
  resetAnsiState(state);

  let html = "";
  let cursor = 0;
  const pattern = /\u001b\[([0-9;]*)m/g;

  for (const match of line.matchAll(pattern)) {
    html += wrapAnsiText(line.slice(cursor, match.index), state);
    applyAnsiCodes(match[1], state);
    cursor = match.index + match[0].length;
  }

  html += wrapAnsiText(line.slice(cursor), state);
  return html;
}

async function call(command, args = {}) {
  if (!invoke) {
    throw new Error("Tauri IPC를 사용할 수 없습니다.");
  }
  return invoke(command, args);
}

async function refreshStatus() {
  const status = await call("bot_status");
  botRunning = status.running;
  fields.toggleBot.textContent = botRunning ? "중지" : "시작";
  fields.toggleBot.dataset.running = String(botRunning);
  setStatus(status.message, status.running ? "running" : "idle");
}

async function refreshLogs() {
  const lines = await call("read_logs");
  fields.logOutput.innerHTML = lines.length
    ? lines.map(renderAnsi).join("\n")
    : escapeHtml("아직 로그가 없습니다.");
  fields.logOutput.scrollTop = fields.logOutput.scrollHeight;
}

async function loadConfig() {
  const config = loadStoredConfig();
  applyConfig(config);
  await refreshStatus();
  await refreshLogs();
}

async function runAction(action) {
  try {
    await action();
  } catch (error) {
    setStatus(String(error), "error");
  }
}

document.querySelector("#toggle-token").addEventListener("click", () => {
  const isPassword = fields.token.type === "password";
  fields.token.type = isPassword ? "text" : "password";
  document.querySelector("#toggle-token").textContent = isPassword ? "숨김" : "보기";
});

document.querySelector("#open-portal").addEventListener("click", () =>
  runAction(() => call("open_url", { url: "https://discord.com/developers/applications" }))
);

document.querySelector("#validate-token").addEventListener("click", () =>
  runAction(async () => {
    setStatus("토큰 확인 중");
    const identity = await call("validate_token", { token: fields.token.value.trim() });
    launcherConfig.client_id = identity.id;
    saveStoredConfig(getConfig());
    setStatus(`${identity.username} 확인됨`, "running");
  })
);

document.querySelector("#invite-bot").addEventListener("click", () =>
  runAction(async () => {
    if (!launcherConfig.client_id) {
      setStatus("토큰 확인 중");
      const identity = await call("validate_token", { token: fields.token.value.trim() });
      launcherConfig.client_id = identity.id;
      saveStoredConfig(getConfig());
    }
    const url = await call("invite_url", { clientId: launcherConfig.client_id });
    await call("open_url", { url });
  })
);

document.querySelector("#toggle-bot").addEventListener("click", () =>
  runAction(async () => {
    if (botRunning) {
      await call("stop_bot");
    } else {
      const config = getConfig();
      saveStoredConfig(config);
      setStatus("시작 중");
      await call("start_bot", { config });
    }
    await refreshStatus();
    await refreshLogs();
  })
);

document.querySelector("#clear-local-data").addEventListener("click", () =>
  runAction(async () => {
    if (!window.confirm("로컬 설정, 로그, DB를 삭제할까요?")) {
      return;
    }
    const status = await call("clear_local_data");
    clearStoredConfig();
    applyConfig(normalizeConfig());
    await refreshStatus();
    setStatus(status.message);
    await refreshLogs();
  })
);

document.querySelector("#open-data-dir").addEventListener("click", () =>
  runAction(() => call("open_data_dir"))
);

document.querySelector("#settings-form").addEventListener("input", (event) => {
  if (event.target === fields.token) {
    launcherConfig.client_id = "";
  }
  saveStoredConfig(getConfig());
});

setInterval(() => runAction(refreshStatus), 3000);
setInterval(() => runAction(refreshLogs), 2000);

loadConfig().catch((error) => setStatus(String(error), "error"));
