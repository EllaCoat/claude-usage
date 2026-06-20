import { invoke } from "@tauri-apps/api/core";

type ModelBreakdown = {
  model: string;
  message_count: number;
  input_tokens: number;
  output_tokens: number;
  cache_creation_tokens: number;
  cache_read_tokens: number;
  cost_usd: number;
};

type UsageSummary = {
  window_start: string | null;
  window_end: string | null;
  now: string;
  elapsed_seconds: number;
  remaining_seconds: number;
  window_progress: number;
  total_messages: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cache_creation_tokens: number;
  total_cache_read_tokens: number;
  total_cost_usd: number;
  by_model: ModelBreakdown[];
};

type OfficialUsage = {
  session_pct: number | null;
  session_reset: string | null;
  week_all_pct: number | null;
  week_all_reset: string | null;
  week_sonnet_pct: number | null;
  fetched_at: string;
};

const LOCAL_REFRESH_MS = 2_000;
const OFFICIAL_REFRESH_MS = 120_000;
const REMOTE_REFRESH_MS = 30_000;
const REMOTE_HOST = "ella-mc";

const nfmt = new Intl.NumberFormat();

const fmtCost = (v: number) => `$${v.toFixed(2)}`;

const fmtTokens = (v: number) => {
  if (v >= 1_000_000) return `${(v / 1_000_000).toFixed(2)}M`;
  if (v >= 1_000) return `${(v / 1_000).toFixed(1)}k`;
  return nfmt.format(v);
};

const fmtDuration = (s: number) => {
  if (s <= 0) return "0s";
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${sec}s`;
  return `${sec}s`;
};

const fmtTime = (iso: string | null) => {
  if (!iso) return "—";
  return new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
};

// claude --print /usage が "Jun 20, 10:30pm (Etc/GMT-9)" のように
// POSIX 表記の timezone を返してくる (Etc/GMT-9 は実は JST = UTC+9 だが
// 見た目で UTC-9 と誤読される)。 frontend では JST/UTC ラベルに置換して表示。
const prettyReset = (s: string | null): string | null => {
  if (!s) return null;
  return s
    .replace(/Etc\/GMT-9\b/g, "JST")
    .replace(/Etc\/UTC\b|Etc\/GMT\b/g, "UTC");
};

const $ = (sel: string) => {
  const el = document.querySelector(sel);
  if (!el) throw new Error(`missing element: ${sel}`);
  return el as HTMLElement;
};

function renderByModel(tableSel: string, by_model: ModelBreakdown[]) {
  const tbody = $(`${tableSel} tbody`);
  tbody.replaceChildren();
  for (const m of by_model) {
    const tr = document.createElement("tr");
    const total =
      m.input_tokens + m.output_tokens + m.cache_creation_tokens + m.cache_read_tokens;
    const cells: Array<[string, string]> = [
      ["mono", m.model],
      ["num", nfmt.format(m.message_count)],
      ["num", fmtTokens(total)],
      ["num", fmtCost(m.cost_usd)],
    ];
    for (const [cls, text] of cells) {
      const td = document.createElement("td");
      td.className = cls;
      td.textContent = text;
      tr.appendChild(td);
    }
    tbody.appendChild(tr);
  }
}

function renderJsonl(prefix: string, s: UsageSummary) {
  const pct = (s.window_progress * 100).toFixed(1);
  $(`#${prefix}-progress-fill`).style.width = `${pct}%`;
  $(`#${prefix}-progress-label`).textContent =
    s.total_messages === 0
      ? "no active 5h window"
      : `${pct}% elapsed · ${fmtDuration(s.remaining_seconds)} until reset`;
  $(`#${prefix}-window-start`).textContent = `start ${fmtTime(s.window_start)}`;
  $(`#${prefix}-window-end`).textContent = `reset ${fmtTime(s.window_end)}`;
  $(`#${prefix}-cost`).textContent = fmtCost(s.total_cost_usd);
  $(`#${prefix}-messages`).textContent = nfmt.format(s.total_messages);
  $(`#${prefix}-tokens`).textContent =
    `in ${fmtTokens(s.total_input_tokens)} · out ${fmtTokens(s.total_output_tokens)} · ` +
    `cache w ${fmtTokens(s.total_cache_creation_tokens)} · r ${fmtTokens(s.total_cache_read_tokens)}`;
  renderByModel(`#${prefix}-by-model`, s.by_model);
}

function setStatus(sel: string, msg: string, isError = false) {
  const el = $(sel);
  el.textContent = msg;
  el.classList.toggle("error", isError);
}

// ---- Local jsonl ----
let localInFlight = false;
async function refreshLocal() {
  if (localInFlight) return;
  localInFlight = true;
  try {
    const s = await invoke<UsageSummary>("get_usage_summary");
    renderJsonl("local", s);
    setStatus("#local-status", `updated ${new Date(s.now).toLocaleTimeString()}`);
  } catch (e) {
    setStatus("#local-status", `error: ${e}`, true);
  } finally {
    localInFlight = false;
  }
}

// ---- Official usage ----
let officialInFlight = false;
async function refreshOfficial() {
  if (officialInFlight) return;
  officialInFlight = true;
  setStatus("#official-status", "fetching…");
  try {
    const o = await invoke<OfficialUsage>("get_official_usage");
    const setBar = (fillId: string, textId: string, pct: number | null) => {
      const p = pct ?? 0;
      $(fillId).style.width = `${p}%`;
      $(textId).textContent = pct === null ? "—" : `${pct}%`;
    };
    setBar("#official-session-fill", "#official-session-text", o.session_pct);
    setBar("#official-week-fill", "#official-week-text", o.week_all_pct);
    setBar("#official-sonnet-fill", "#official-sonnet-text", o.week_sonnet_pct);

    const lines: string[] = [];
    if (o.session_reset) lines.push(`session resets ${prettyReset(o.session_reset)}`);
    if (o.week_all_reset) lines.push(`week resets ${prettyReset(o.week_all_reset)}`);
    $("#official-reset").textContent = lines.join(" · ") || "—";

    setStatus(
      "#official-status",
      `updated ${new Date(o.fetched_at).toLocaleTimeString()}`,
    );
  } catch (e) {
    setStatus("#official-status", `error: ${e}`, true);
  } finally {
    officialInFlight = false;
  }
}

// ---- Remote jsonl ----
let remoteInFlight = false;
async function refreshRemote() {
  if (remoteInFlight) return;
  remoteInFlight = true;
  setStatus("#remote-status", "fetching via ssh…");
  try {
    const s = await invoke<UsageSummary>("get_remote_jsonl_summary", { host: REMOTE_HOST });
    renderJsonl("remote", s);
    setStatus("#remote-status", `updated ${new Date(s.now).toLocaleTimeString()}`);
  } catch (e) {
    setStatus("#remote-status", `error: ${e}`, true);
  } finally {
    remoteInFlight = false;
  }
}

window.addEventListener("DOMContentLoaded", () => {
  $("#remote-host-label").textContent = REMOTE_HOST;

  $("#refresh-all").addEventListener("click", () => {
    refreshLocal();
    refreshOfficial();
    refreshRemote();
  });

  refreshLocal();
  refreshOfficial();
  refreshRemote();

  setInterval(refreshLocal, LOCAL_REFRESH_MS);
  setInterval(refreshOfficial, OFFICIAL_REFRESH_MS);
  setInterval(refreshRemote, REMOTE_REFRESH_MS);
});
