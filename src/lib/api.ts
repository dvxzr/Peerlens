import { Channel, invoke } from "@tauri-apps/api/core";
import type { CacheInfo, UpdateInfo, UpdateProgress, AgentInfo, AgentKind, ReviewEvent, StoredReview, ThreadEvent, Usage, Commit, Discussion, FileDiff, MrDetails, MrSummary, Settings, User } from "./types";

type Id = { projectId: number; iid: number };

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (s: Settings) => invoke<void>("save_settings", { new: s }),
  listMyMrs: (all: boolean) => invoke<{ me: User; mrs: MrSummary[] }>("list_my_mrs", { all }),
  details: (id: Id) => invoke<MrDetails>("mr_details", id),
  commits: (id: Id) => invoke<Commit[]>("mr_commits", id),
  diffs: (id: Id) => invoke<FileDiff[]>("mr_diffs", id),
  commitDiff: (projectId: number, sha: string) => invoke<FileDiff[]>("commit_diff", { projectId, sha }),
  discussions: (id: Id) => invoke<Discussion[]>("mr_discussions", id),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  detectAgents: () => invoke<AgentInfo[]>("detect_agents"),
  loadReview: (id: Id) => invoke<StoredReview | null>("load_review", id),
  cancelReview: () => invoke<void>("cancel_review"),
  getUsage: () => invoke<Usage | null>("get_usage"),
  deleteReview: (id: Id) => invoke<void>("delete_review", id),
  discussFinding: (
    id: Id, index: number, message: string, agent: AgentKind, model: string, effort: string, repoContext: boolean,
    onEvent: (e: ThreadEvent) => void,
  ) => {
    const ch = new Channel<ThreadEvent>();
    ch.onmessage = onEvent;
    return invoke<void>("discuss_finding", { ...id, index, message, agent, model, effort, repoContext, onEvent: ch });
  },
  repoCacheInfo: () => invoke<CacheInfo>("repo_cache_info"),
  clearRepoCache: () => invoke<void>("clear_repo_cache"),
  deleteThread: (id: Id, index: number) => invoke<void>("delete_thread", { ...id, index }),
  setFindingStatus: (id: Id, index: number, status: string) => invoke<void>("set_finding_status", { ...id, index, status }),
  defaultGuidelines: () => invoke<string>("default_guidelines"),
  appVersion: () => invoke<string>("app_version"),
  checkUpdate: () => invoke<UpdateInfo | null>("check_update"),
  installUpdate: (onProgress: (p: UpdateProgress) => void) => {
    const ch = new Channel<UpdateProgress>();
    ch.onmessage = onProgress;
    return invoke<void>("install_update", { onProgress: ch });
  },
  startReview: (
    id: Id, agent: AgentKind, model: string, effort: string, scopeFile: string | null, language: string, repoContext: boolean,
    notes: string, since: string | null, onEvent: (e: ReviewEvent) => void,
  ) => {
    const ch = new Channel<ReviewEvent>();
    ch.onmessage = onEvent;
    return invoke<void>("start_review", { ...id, agent, model, effort, scopeFile, language, repoContext, notes, since, onEvent: ch });
  },
};

export const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e));
