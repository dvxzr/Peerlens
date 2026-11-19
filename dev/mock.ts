// Fake Tauri backend for previewing the UI in a plain browser. Not part of the app bundle.
const long = "    result = some_function_with_a_long_name(argument_one, argument_two, argument_three, keyword_argument=compute_default_value(), another_keyword=None)  # trailing comment that keeps going";
const pyDiff = [
  "@@ -1,8 +1,12 @@ class RateLimiter:",
  " import time",
  " from collections import defaultdict",
  "-def login(user, password):",
  "+def login(user, password, attempts={}):",
  "+    attempts[user] = attempts.get(user, 0) + 1",
  "+    if attempts[user] > MAX_ATTEMPTS:",
  "+        raise PermissionError(f\"too many attempts for {user}\")",
  "     return check(user, password)",
  " ",
  "+" + long,
  " ",
  "@@ -40,6 +44,6 @@ def check(user, password):",
  "     row = db.fetch_one(\"SELECT * FROM users WHERE name = '%s'\" % user)",
  "-    return row and row.password == password",
  "+    return row is not None and hmac.compare_digest(row.password, password)",
  "     # end",
].join("\n");
const tsDiff = "@@ -1,3 +1,4 @@\n import { x } from \"./x\";\n-export const y = 1;\n+export const y: number = 2;\n+export function z(a: string): string { return `${a}!`; }\n";
const files = [
  { old_path: "src/auth/login.py", new_path: "src/auth/login.py", diff: pyDiff },
  { old_path: "web/src/lib/util.ts", new_path: "web/src/lib/util.ts", diff: tsDiff },
  { old_path: "poetry.lock", new_path: "poetry.lock", diff: "@@ -1 +1 @@\n-a\n+b\n", generated: true },
  { old_path: "assets/logo.png", new_path: "assets/logo.png", diff: "", new_file: true },
];

function parse(diff: string) {
  const out: any[] = [];
  let o = 0, n = 0, inH = false;
  for (const l of diff.split("\n")) {
    const m = /^@@ -(\d+)(?:,\d+)? \+(\d+)/.exec(l);
    if (m) { o = +m[1]; n = +m[2]; inH = true; out.push({ k: "@", o: null, n: null, t: l }); continue; }
    if (!inH || !l) continue;
    const k = l[0], t = l.slice(1);
    if (k === "+") out.push({ k, o: null, n: n++, t });
    else if (k === "-") out.push({ k, o: o++, n: null, t });
    else out.push({ k: " ", o: o++, n: n++, t });
  }
  return out;
}
// Repeat the Python file so the diff is long enough to test scrolling and the pinned header.
for (let i = 2; i <= 5; i++) files.splice(1, 0, { old_path: `src/auth/login_v${i}.py`, new_path: `src/auth/login_v${i}.py`, diff: pyDiff });
const fileDiffs = files.map((f: any) => {
  const lines = parse(f.diff);
  return {
    old_path: f.old_path, new_path: f.new_path, new_file: !!f.new_file, renamed_file: false, deleted_file: false,
    too_large: false, collapsed: false, generated: !!f.generated, binary: !f.diff,
    additions: lines.filter((l) => l.k === "+").length, deletions: lines.filter((l) => l.k === "-").length, lines,
  };
});
const user = (u: string, name: string) => ({ id: 1, username: u, name });
const mr = (pid: number, iid: number, title: string, proj: string, draft = false, a = true, r = false) => ({
  project_id: pid, iid, title, web_url: "https://gitlab.example.com", author: user("alice.dev", "Alice Developer"),
  source_branch: `feature/SHOP-${iid}`, target_branch: "main", updated_at: new Date(Date.now() - iid * 3.6e5).toISOString(),
  draft, sha: "abc123", references: { full: `${proj}!${iid}` }, user_notes_count: 3, is_assignee: a, is_reviewer: r,
});
const mrs = [
  mr(1, 1130, "SHOP-4248 Rate-limit login attempts", "acme/shop/api-server"),
  mr(1, 1148, "Draft: SHOP-4646: Add account merge flow for users with a very long title that wraps", "acme/shop/api-server", true, false, true),
  mr(2, 295, "SHOP-3371 Add bulk edit endpoints for product prices", "acme/shop/catalog", false, false, true),
  mr(3, 5, "refactor(auth): streamline authentication middleware", "acme/platform/auth", false, true, true),
];
const settings = {
  glab_path: "", host: "", claude_path: "", codex_path: "", default_agent: "claude", review_language: "English",
  max_review_bytes: 120000, collapse_lines: 800, agent_timeout_secs: 900, theme: new URLSearchParams(location.search).get("theme") ?? "dark",
  default_model: "sonnet", default_effort: "", default_codex_model: "", chunk_delay_secs: 3, max_chunks: 25, max_reviews_per_hour: 20, max_usage_pct: 80,
  repo_max_mb: 300, repo_exclude: [".env", "*.pem"], review_guidelines: "",
};
const usage = { status: "allowed", windows: [
  { name: "five_hour", utilization: 0.42, resets_at: Date.now() / 1000 + 7200, status: "allowed" },
  { name: "seven_day", utilization: 0.38, resets_at: Date.now() / 1000 + 86400 * 3, status: "allowed" },
] };
const review = {
  project_id: 1, iid: 1130, head_sha: "abc123", agent: "Claude", model: "sonnet", language: "English", scope: "whole MR",
  created_at: Date.now() / 1000 - 600, cancelled: false, raw: "", errors: [], usage,
  skipped: [{ path: "poetry.lock", reason: "lock file" }, { path: "assets/logo.png", reason: "binary or empty" }],
  activity: ["Grep MAX_ATTEMPTS  in .", "Read src/auth/settings.py", "Glob **/test_login*.py"],
  repo: { files: 552, bytes: 5213243, excluded: [".env"], skipped_large: 0, skipped_links: 0 },
  summaries: ["Needs changes: the limiter keeps its state in a mutable default argument and the user lookup builds SQL with string formatting. Fix both before merging."],
  findings: [
    { file: "src/auth/login.py", start_line: 3, end_line: 4, severity: "critical", category: "bug", title: "Mutable default argument keeps state across calls", explanation: "`attempts={}` is created once and shared by every call and every user, so the counter never resets and one user can lock out others.", snippet: "def login(user, password, attempts={}):", suggestion: { description: "Keep the counter in a store with a TTL and pass it in.", code: "def login(user, password, store: AttemptStore):\n    if store.incr(user) > MAX_ATTEMPTS:\n        raise PermissionError(...)" }, anchored: true },
    { file: "src/auth/login.py", start_line: 45, end_line: 45, severity: "major", category: "security", title: "SQL injection through string formatting", explanation: "`user` is interpolated into the query, so a crafted name can change the SQL.", snippet: null, suggestion: { description: "Use a parameterized query.", code: "db.fetch_one(\"SELECT * FROM users WHERE name = %s\", (user,))" }, anchored: true },
    { file: "web/src/lib/util.ts", start_line: null, end_line: null, severity: "nit", category: "style", title: "Redundant type annotation", explanation: "The type is inferred from the literal.", snippet: null, suggestion: null, anchored: false },
  ],
};
const handlers: Record<string, (a: any) => any> = {
  get_settings: () => settings,
  save_settings: () => null,
  list_my_mrs: () => new Promise((r) => setTimeout(() => r({ me: user("me", "Me"), mrs }), 300)),
  mr_details: (a) => ({ ...mrs.find((m) => m.iid === a.iid), description: "Adds a limiter.\n\nSecond paragraph.", state: "opened", detailed_merge_status: "mergeable", head_pipeline: { status: "failed", web_url: "" }, diff_refs: { base_sha: "0", head_sha: "abc123", start_sha: "0" }, source_project_id: a.projectId }),
  mr_diffs: () => new Promise((r) => setTimeout(() => r(fileDiffs), 200)),
  mr_commits: () => [{ id: "abc123def", short_id: "abc123d", title: "Add limiter", author_name: "Alice", authored_date: new Date().toISOString(), web_url: "" }, { id: "fed321", short_id: "fed321c", title: "Tests for limiter and a very long commit title to check truncation in the list", author_name: "Alice", authored_date: new Date().toISOString(), web_url: "" }],
  commit_diff: () => fileDiffs.slice(0, 1),
  mr_discussions: () => [
    { id: "d1", individual_note: false, notes: [{ id: 1, body: "Mutable default arg!", author: user("bob", "Bob Builder"), created_at: new Date().toISOString(), system: false, resolvable: true, resolved: false, position: { new_path: "src/auth/login.py", old_path: "src/auth/login.py", new_line: 3, old_line: null } }, { id: 2, body: "Will fix", author: user("alice.dev", "Alice Developer"), created_at: new Date().toISOString(), system: false, resolvable: true, resolved: false, position: null }] },
    { id: "d0", individual_note: true, notes: [{ id: 9, body: "## SonarQube Cloud Code Analysis\n## Quality Gate passed\n**Issues**\n![](https://sonarsource.github.io/sonarcloud-github-static-resources/v2/common/passed.svg '') [0 New issues](https://sonarcloud.io/project/issues?id=x)\n\n**Measures**\n![](https://sonarsource.github.io/x/accepted.svg '') [0 Accepted issues](https://sonarcloud.io/a)\n![](https://sonarsource.github.io/x/passed.svg '') [100.0% Coverage on New Code](https://sonarcloud.io/m)\n![](https://sonarsource.github.io/x/failed.svg '') [2 Security Hotspots](https://sonarcloud.io/h)\n\n:white_check_mark: Looks good :tada: :thumbsup: 🚀\n\n| Check | Result |\n|---|---|\n| Lint | **ok** |\n| Tests | ~~flaky~~ fixed |\n\n- [x] migrated\n- [ ] docs\n  - nested `item`\n1. first\n2. second\n\n> quoted *text* with a [link](https://example.com)\n\n```python\nprint(\"<b>not bold</b>\")\n```\n<script>alert(1)</script> <img src=x onerror=alert(2)> [evil](javascript:alert(3)) <details><summary>More</summary>hidden text</details>", author: user("group_1001_bot_0123456789abcdef", "quality-bot"), created_at: new Date().toISOString(), system: false, resolvable: false, resolved: false, position: null }] },
    { id: "d2", individual_note: true, notes: [{ id: 3, body: "LGTM <b>not html</b>", author: user("carol", "Carol"), created_at: new Date().toISOString(), system: false, resolvable: true, resolved: true, position: null }] },
  ],
  open_url: () => null,
  detect_agents: () => [
    { kind: "claude", name: "Claude Code", path: "/Users/me/.local/bin/claude", version: "", logged_in: null, models: [] },
    { kind: "codex", name: "Codex", path: "/opt/homebrew/bin/codex", version: "codex-cli 0.170.0", logged_in: new URLSearchParams(location.search).has("codexlogin"), models: [
      { slug: "gpt-6.2-sol", name: "GPT-6.2-Sol", description: "Fast, go-to model for coding and everyday work.", efforts: ["low", "medium", "high", "xhigh", "max"], default_effort: "low" },
      { slug: "gpt-6-luna", name: "GPT-6-Luna", description: "Fast and affordable model for easier tasks.", efforts: ["low", "medium", "high"], default_effort: "medium" },
    ] },
  ],
  load_review: (a) => (a.iid === 1130 ? review : null),
  get_usage: () => usage,
  cancel_review: () => null,
  delete_review: () => null,
  default_guidelines: () => "You are a principal engineer reviewing a merge request.\n\nWhat to check, in priority order:\n1. Correctness…\n\nRules:\n- Report only problems introduced by this change…",
  start_review: () => new Promise(() => {}),
};
let cbId = 0;
(window as any).__TAURI_INTERNALS__ = {
  transformCallback: (cb: any) => { const id = ++cbId; (window as any)[`_${id}`] = cb; return id; },
  invoke: async (cmd: string, args: any) => {
    const h = handlers[cmd];
    if (!h) throw `mock: no handler for ${cmd}`;
    return h(args);
  },
};
await import("../src/main");
