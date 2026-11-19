import type { IconName } from "./icons";

export interface User { id: number; username: string; name: string }
export interface MrSummary {
  project_id: number; iid: number; title: string; web_url: string; author: User;
  source_branch: string; target_branch: string; updated_at: string; draft: boolean;
  sha: string | null; references: { full: string }; user_notes_count: number;
  is_assignee: boolean; is_reviewer: boolean;
}
export interface MrDetails {
  project_id: number; iid: number; title: string; description: string | null; state: string;
  web_url: string; author: User; source_branch: string; target_branch: string; draft: boolean;
  detailed_merge_status: string | null; head_pipeline: { status: string; web_url: string } | null;
  diff_refs: { base_sha: string; head_sha: string; start_sha: string } | null; sha: string | null;
  references: { full: string };
}
export interface Commit { id: string; short_id: string; title: string; author_name: string; authored_date: string; web_url: string }
export interface Line { k: "@" | "+" | "-" | " " | "\\"; o: number | null; n: number | null; t: string }
export interface FileDiff {
  old_path: string; new_path: string; new_file: boolean; renamed_file: boolean; deleted_file: boolean;
  too_large: boolean; collapsed: boolean; generated: boolean; binary: boolean;
  additions: number; deletions: number; lines: Line[];
}
export interface Position { new_path: string | null; old_path: string | null; new_line: number | null; old_line: number | null }
export interface Note {
  id: number; body: string; author: User; created_at: string; system: boolean;
  resolvable: boolean; resolved: boolean; position: Position | null;
}
export interface Discussion { id: string; individual_note: boolean; notes: Note[] }
export interface Settings {
  glab_path: string; host: string; claude_path: string; codex_path: string; default_agent: string;
  review_language: string; max_review_bytes: number; collapse_lines: number; agent_timeout_secs: number;
  theme: "dark" | "light" | "system"; default_model: string; default_effort: string; default_codex_model: string; chunk_delay_secs: number; max_chunks: number;
  max_reviews_per_hour: number; max_usage_pct: number; repo_max_mb: number; repo_exclude: string[];
  review_guidelines: string;
}
export type Marker = { severity: string; title: string; id: number };
export type AgentKind = "claude" | "codex";
export interface AgentModel { slug: string; name: string; description: string; efforts: string[]; default_effort: string }
export interface AgentInfo { kind: AgentKind; name: string; path: string; version: string; logged_in: boolean | null; models: AgentModel[] }
export interface Finding {
  file: string; start_line: number | null; end_line: number | null; severity: string; category: string;
  title: string; explanation: string; snippet: string | null;
  suggestion: { description: string; code: string } | null; anchored: boolean;
}
export interface Skipped { path: string; reason: string }
export interface StoredReview {
  project_id: number; iid: number; head_sha: string; agent: string; model: string; effort: string; language: string; scope: string;
  created_at: number; findings: Finding[]; summaries: string[]; raw: string; skipped: Skipped[]; errors: string[]; cancelled: boolean;
  usage: Usage | null;
  activity: string[];
  repo: SnapshotStats | null;
}
export interface SnapshotStats { files: number; bytes: number; excluded: string[]; skipped_large: number; skipped_links: number }
export interface UsageWindow { name: string; utilization: number; resets_at: number; status: string }
export interface Usage { status: string; windows: UsageWindow[] }
export type ReviewEvent =
  | { type: "started"; chunks: number; skipped: Skipped[] }
  | { type: "progress"; chunk: number; total: number; files: string[] }
  | { type: "finding"; finding: Finding }
  | { type: "summary"; text: string }
  | { type: "usage"; usage: Usage }
  | { type: "repo"; stage: "downloading" | "ready"; stats: SnapshotStats | null }
  | { type: "activity"; text: string }
  | { type: "error"; message: string }
  | { type: "done"; review: StoredReview };

export type MenuItem<V> = { value: V; label: string; desc?: string; icon?: IconName; color?: string; disabled?: boolean };
