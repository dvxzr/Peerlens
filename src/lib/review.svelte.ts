import { api, errText } from "./api";
import type { AgentKind, Finding, Skipped, SnapshotStats, StoredReview, ThreadMsg, Usage } from "./types";

export interface ReviewState {
  running: boolean;
  progress: { chunk: number; total: number; files: string[] } | null;
  findings: Finding[];
  summaries: string[];
  skipped: Skipped[];
  errors: string[];
  raw: string;
  headSha: string;
  createdAt: number | null;
  cancelled: boolean;
  meta: string;
  repoStage: "" | "downloading" | "ready";
  repo: SnapshotStats | null;
  activity: string[];
  /** Discussion threads keyed by finding index. */
  threads: Record<number, ThreadMsg[]>;
  /** The thread reply being written right now. */
  chat: { index: number; message: string; reply: string; repo: "" | "downloading" | "ready"; activity: string[]; notice: string } | null;
  chatError: { index: number; message: string } | null;
}

// Latest Claude subscription usage, shared by all panels.
export const usage: { value: Usage | null } = $state({ value: null });
export async function loadUsage() {
  try { usage.value = (await api.getUsage()) ?? usage.value; } catch { /* not available yet */ }
}

const empty = (): ReviewState => ({
  running: false, progress: null, findings: [], summaries: [], skipped: [], errors: [], raw: "",
  headSha: "", createdAt: null, cancelled: false, meta: "", repoStage: "", repo: null, activity: [],
  threads: {}, chat: null, chatError: null,
});

// One review state per MR, kept in memory so switching MRs doesn't lose a running review.
export const reviews: Record<string, ReviewState> = $state({});
const key = (pid: number, iid: number) => `${pid}:${iid}`;

function fromStored(r: StoredReview): ReviewState {
  return {
    ...empty(), findings: r.findings, summaries: r.summaries, skipped: r.skipped, errors: r.errors, raw: r.raw,
    headSha: r.head_sha, createdAt: r.created_at * 1000, cancelled: r.cancelled,
    meta: `${r.agent}${r.model ? " / " + r.model : ""}${r.effort ? " / " + r.effort : ""} · ${r.language} · ${r.scope}${r.repo ? " · repo context" : ""}`,
    repo: r.repo ?? null, repoStage: r.repo ? "ready" : "", activity: r.activity ?? [], threads: r.threads ?? {},
  };
}

export async function ensureLoaded(pid: number, iid: number) {
  const k = key(pid, iid);
  if (reviews[k]) return;
  reviews[k] = empty();
  try {
    const r = await api.loadReview({ projectId: pid, iid });
    if (r && !reviews[k].running) reviews[k] = fromStored(r);
  } catch {
    /* no stored review */
  }
}

export function get(pid: number, iid: number): ReviewState | undefined {
  return reviews[key(pid, iid)];
}

export async function start(
  pid: number, iid: number, agent: AgentKind, model: string, effort: string, scopeFile: string | null, language: string,
  repoContext: boolean, notes: string,
) {
  const k = key(pid, iid);
  reviews[k] = { ...empty(), running: true };
  const s = reviews[k];
  try {
    await api.startReview({ projectId: pid, iid }, agent, model, effort, scopeFile, language, repoContext, notes, (e) => {
      switch (e.type) {
        case "started": s.skipped = e.skipped; break;
        case "progress": s.progress = { chunk: e.chunk, total: e.total, files: e.files }; break;
        case "finding": s.findings.push(e.finding); break;
        case "summary": s.summaries.push(e.text); break;
        case "error": s.errors.push(e.message); break;
        case "usage": usage.value = e.usage; break;
        case "repo": s.repoStage = e.stage; s.repo = e.stats; break;
        case "activity": s.activity.push(e.text); break;
        case "done": reviews[k] = fromStored(e.review); break;
      }
    });
  } catch (e) {
    s.errors.push(errText(e));
  } finally {
    reviews[k].running = false;
  }
}

export async function remove(pid: number, iid: number) {
  await api.deleteReview({ projectId: pid, iid });
  reviews[key(pid, iid)] = empty();
}

/** Send one message in a finding's thread. Resolves true when the reply was saved. */
export async function discuss(
  pid: number, iid: number, index: number, message: string, agent: AgentKind, model: string, effort: string, repoContext: boolean,
): Promise<boolean> {
  const s = reviews[key(pid, iid)];
  if (!s || s.running || s.chat) return false;
  s.chat = { index, message, reply: "", repo: "", activity: [], notice: "" };
  s.chatError = null;
  try {
    await api.discussFinding({ projectId: pid, iid }, index, message, agent, model, effort, repoContext, (e) => {
      const c = s.chat;
      if (e.type === "usage") usage.value = e.usage;
      else if (e.type === "done") s.threads[index] = e.thread;
      else if (!c) return;
      else if (e.type === "text") c.reply += e.text;
      else if (e.type === "repo") c.repo = e.stage;
      else if (e.type === "activity") c.activity.push(e.text);
      else if (e.type === "notice") c.notice = e.text;
    });
    return true;
  } catch (e) {
    const m = errText(e);
    if (m !== "Cancelled") s.chatError = { index, message: m };
    return false;
  } finally {
    s.chat = null;
  }
}

export async function removeThread(pid: number, iid: number, index: number) {
  await api.deleteThread({ projectId: pid, iid }, index);
  const s = reviews[key(pid, iid)];
  if (s) delete s.threads[index];
}
