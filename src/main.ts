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

const REFRESH_MS = 2000;
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

const $ = (sel: string) => {
  const el = document.querySelector(sel);
  if (!el) throw new Error(`missing element: ${sel}`);
  return el as HTMLElement;
};

async function refresh() {
  try {
    const s = await invoke<UsageSummary>("get_usage_summary");
    const pct = (s.window_progress * 100).toFixed(1);

    $("#progress-fill").style.width = `${pct}%`;
    $("#progress-label").textContent =
      s.total_messages === 0
        ? "no activity in last 5h"
        : `${pct}% elapsed · ${fmtDuration(s.remaining_seconds)} until reset`;
    $("#window-start").textContent = `start ${fmtTime(s.window_start)}`;
    $("#window-end").textContent = `reset ${fmtTime(s.window_end)}`;

    $("#cost").textContent = fmtCost(s.total_cost_usd);
    $("#messages").textContent = nfmt.format(s.total_messages);
    $("#tokens").textContent =
      `in ${fmtTokens(s.total_input_tokens)} · out ${fmtTokens(s.total_output_tokens)} · ` +
      `cache write ${fmtTokens(s.total_cache_creation_tokens)} · read ${fmtTokens(s.total_cache_read_tokens)}`;

    const tbody = $("#by-model tbody");
    tbody.innerHTML = "";
    for (const m of s.by_model) {
      const tr = document.createElement("tr");
      const total = m.input_tokens + m.output_tokens + m.cache_creation_tokens + m.cache_read_tokens;
      tr.innerHTML = `
        <td class="mono">${m.model}</td>
        <td class="num">${nfmt.format(m.message_count)}</td>
        <td class="num">${fmtTokens(total)}</td>
        <td class="num">${fmtCost(m.cost_usd)}</td>
      `;
      tbody.appendChild(tr);
    }

    $("#status").textContent = `updated ${new Date(s.now).toLocaleTimeString()}`;
    $("#status").classList.remove("error");
  } catch (e) {
    $("#status").textContent = `error: ${e}`;
    $("#status").classList.add("error");
  }
}

window.addEventListener("DOMContentLoaded", () => {
  refresh();
  setInterval(refresh, REFRESH_MS);
});
