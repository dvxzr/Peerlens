// Fake Tauri backend for previewing the UI in a plain browser. Not part of the app bundle.
// Realistic, fictional data: the README screenshots (docs/*.png) are taken from this page.
const diffs: Record<string, string> = {
  "src/auth/limiter.py": [
    "@@ -0,0 +1,21 @@",
    "+\"\"\"Login attempt limiter.\"\"\"",
    "+import time",
    "+from collections import defaultdict",
    "+",
    "+MAX_ATTEMPTS = 5",
    "+WINDOW_SECONDS = 15 * 60",
    "+",
    "+",
    "+class RateLimiter:",
    "+    def __init__(self, attempts={}):",
    "+        self.attempts = attempts",
    "+",
    "+    def hit(self, user: str) -> bool:",
    "+        now = time.monotonic()",
    "+        window = [t for t in self.attempts.get(user, []) if now - t < WINDOW_SECONDS]",
    "+        window.append(now)",
    "+        self.attempts[user] = window",
    "+        return len(window) <= MAX_ATTEMPTS",
    "+",
    "+    def reset(self, user: str) -> None:",
    "+        self.attempts.pop(user, None)",
  ].join("\n"),
  "src/auth/login.py": [
    "@@ -1,8 +1,15 @@",
    " import hmac",
    " from db import fetch_one",
    "+from auth.limiter import RateLimiter",
    " ",
    "+limiter = RateLimiter()",
    "+",
    " ",
    "-def login(user, password):",
    "+def login(user: str, password: str) -> bool:",
    "+    if not limiter.hit(user):",
    "+        raise PermissionError(f\"too many attempts for {user}\")",
    "     row = fetch_one(\"SELECT * FROM users WHERE name = '%s'\" % user)",
    "-    return row and row.password == password",
    "+    ok = row is not None and hmac.compare_digest(row.hash, hash_password(password, row.salt))",
    "+    if ok:",
    "+        limiter.reset(user)",
    "+    return ok",
  ].join("\n"),
  "src/config/settings.py": [
    "@@ -18,5 +18,8 @@ SESSION_COOKIE_SECURE = True",
    " SESSION_COOKIE_AGE = 60 * 60 * 24",
    " ",
    "+# Login rate limiting",
    "+LOGIN_MAX_ATTEMPTS = 5",
    "+LOGIN_WINDOW_SECONDS = 900",
    " ",
    " LOGGING = {",
  ].join("\n"),
  "tests/auth/test_limiter.py": [
    "@@ -0,0 +1,16 @@",
    "+import pytest",
    "+from auth.limiter import MAX_ATTEMPTS, RateLimiter",
    "+",
    "+",
    "+def test_blocks_after_max_attempts():",
    "+    limiter = RateLimiter()",
    "+    for _ in range(MAX_ATTEMPTS):",
    "+        assert limiter.hit(\"alice\")",
    "+    assert not limiter.hit(\"alice\")",
    "+",
    "+",
    "+def test_reset_clears_attempts():",
    "+    limiter = RateLimiter()",
    "+    limiter.hit(\"bob\")",
    "+    limiter.reset(\"bob\")",
    "+    assert limiter.hit(\"bob\")",
  ].join("\n"),
  "web/src/lib/api.ts": [
    "@@ -10,7 +10,12 @@ export class RateLimitError extends Error {",
    " export async function login(user: string, password: string) {",
    "   const res = await fetch(\"/api/login\", { method: \"POST\", body: JSON.stringify({ user, password }) });",
    "+  if (res.status === 429) {",
    "+    const retry = Number(res.headers.get(\"Retry-After\") ?? 60);",
    "+    throw new RateLimitError(`Too many attempts, try again in ${retry}s`, retry);",
    "+  }",
    "   if (!res.ok) throw new Error(\"Login failed\");",
    "   return res.json();",
    " }",
  ].join("\n"),
  "poetry.lock": "@@ -1 +1 @@\n-content-hash = \"a1\"\n+content-hash = \"b2\"",
};
const newFiles = new Set(["src/auth/limiter.py", "tests/auth/test_limiter.py"]);

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
const fileDiffs = Object.entries(diffs).map(([path, diff]) => {
  const lines = parse(diff);
  return {
    old_path: path, new_path: path, new_file: newFiles.has(path), renamed_file: false, deleted_file: false,
    too_large: false, collapsed: false, generated: path.endsWith(".lock"), binary: !diff,
    additions: lines.filter((l) => l.k === "+").length, deletions: lines.filter((l) => l.k === "-").length, lines,
  };
});
const user = (u: string, name: string) => ({ id: 1, username: u, name });
const ago = (min: number) => new Date(Date.now() - min * 60_000).toISOString();
const mr = (pid: number, iid: number, title: string, proj: string, author: [string, string], min: number, role: "review" | "mine" | "both", draft = false) => ({
  project_id: pid, iid, title, web_url: "https://gitlab.example.com", author: user(...author),
  source_branch: `feature/${title.split(" ")[0].toLowerCase()}`, target_branch: pid === 2 ? "release/2026.12" : "main", updated_at: ago(min),
  draft, sha: "abc123", references: { full: `${proj}!${iid}` }, user_notes_count: 3, is_assignee: role !== "review", is_reviewer: role !== "mine",
});
const mrs = [
  mr(1, 1130, "SHOP-4248 Rate-limit login attempts", "acme/shop/api-server", ["alice.dev", "Alice Developer"], 25, "review"),
  mr(1, 1148, "Draft: SHOP-4646 Merge duplicate customer profiles", "acme/shop/api-server", ["bob.lee", "Bob Lee"], 180, "review", true),
  mr(1, 1152, "SHOP-4702 Cache product availability per warehouse", "acme/shop/api-server", ["maria.k", "Maria Kowalski"], 60 * 26, "review"),
  mr(2, 295, "SHOP-3371 Bulk edit endpoints for product prices", "acme/shop/catalog", ["dan.ortiz", "Dan Ortiz"], 60 * 5, "review"),
  mr(3, 412, "BILL-981 Retry failed invoice webhooks with backoff", "acme/platform/billing", ["priya.n", "Priya Nair"], 60 * 50, "review"),
  mr(4, 57, "Streamline session refresh middleware", "acme/platform/auth", ["you", "You"], 90, "mine"),
];
const settings = {
  glab_path: "", host: "", claude_path: "", codex_path: "", default_agent: "claude", review_language: "English",
  max_review_bytes: 120000, collapse_lines: 800, agent_timeout_secs: 900, theme: new URLSearchParams(location.search).get("theme") ?? "dark",
  default_model: "sonnet", default_effort: "medium", default_codex_model: "", chunk_delay_secs: 3, max_chunks: 25, max_reviews_per_hour: 20, max_usage_pct: 80,
  repo_max_mb: 300, repo_exclude: [".env", ".env.*", "*.pem", "*.key"], repo_cache_mb: 2048, review_guidelines: "",
};
const usage = { status: "allowed", windows: [
  { name: "five_hour", utilization: 0.34, resets_at: Date.now() / 1000 + 7800, status: "allowed" },
  { name: "seven_day", utilization: 0.21, resets_at: Date.now() / 1000 + 86400 * 3, status: "allowed" },
] };
const finding = (file: string, line: number | null, severity: string, category: string, title: string, explanation: string, snippet: string | null = null, suggestion: { description: string; code: string } | null = null) =>
  ({ file, start_line: line, end_line: line, severity, category, title, explanation, snippet, suggestion, anchored: line != null });
const review = {
  project_id: 1, iid: 1130, head_sha: "abc123", agent: "Claude", model: "sonnet", effort: "medium", language: "English", scope: "whole MR",
  created_at: Date.now() / 1000 - 600, cancelled: false, raw: "", errors: [], usage,
  skipped: [{ path: "poetry.lock", reason: "lock file" }],
  activity: ["Grep MAX_ATTEMPTS in .", "Read src/auth/session.py", "Glob tests/**/test_login*.py"],
  repo: { files: 552, bytes: 5213243, excluded: [".env"], skipped_large: 0, skipped_links: 0 },
  threads: {
    0: [
      { role: "user", text: "The limiter is created once per process anyway. Does the default really matter?", at: Date.now() / 1000 - 300, by: "" },
      { role: "agent", by: "Claude / sonnet", at: Date.now() / 1000 - 280, text: "**Still valid.** `attempts={}` is one dict shared by every `RateLimiter()`, not only the module-level one:\n\n- the two tests in `test_limiter.py` share state, so their result depends on the order they run in;\n- any second limiter (e.g. for password reset) silently counts the same users.\n\nPass `attempts=None` and create the dict inside `__init__`." },
    ],
  },
  summaries: ["The limiter works for the happy path, but its state leaks between instances through a mutable default argument, and the user lookup still builds SQL with string formatting. Fix both before merging."],
  findings: [
    finding("src/auth/limiter.py", 10, "critical", "bug", "Mutable default argument shares state between limiters",
      "`attempts={}` is evaluated once, so every `RateLimiter()` without an argument shares the same dict: tests and other limiters count each other's attempts.",
      "def __init__(self, attempts={}):",
      { description: "Default to `None` and create the dict per instance.", code: "def __init__(self, attempts: dict[str, list[float]] | None = None):\n    self.attempts = attempts if attempts is not None else {}" }),
    finding("src/auth/login.py", 11, "major", "security", "User lookup builds SQL with string formatting",
      "`user` is interpolated into the query, so a crafted name can change the SQL. The new limiter key comes from the same unvalidated value.",
      null, { description: "Use a parameterized query.", code: "row = fetch_one(\"SELECT * FROM users WHERE name = %s\", (user,))" }),
    finding("src/auth/limiter.py", 15, "minor", "performance", "Idle users are never evicted",
      "Entries are pruned only when the same user logs in again, so the dict grows with every distinct user name, including random ones from attackers."),
    finding("src/config/settings.py", 21, "minor", "design", "New settings are never read",
      "`LOGIN_MAX_ATTEMPTS` and `LOGIN_WINDOW_SECONDS` are defined, but the limiter uses its own module constants."),
    finding("web/src/lib/api.ts", 13, "minor", "bug", "Client waits for 429, server raises PermissionError",
      "The backend maps `PermissionError` to 403, so the Retry-After branch never runs and users see \"Login failed\"."),
    finding("tests/auth/test_limiter.py", 1, "nit", "style", "Unused import", "`pytest` is imported but not used."),
  ],
};
const small = (iid: number, sevs: string[]) => ({
  ...review, iid, threads: {}, activity: [], repo: null, summaries: ["Small, focused change."],
  findings: sevs.map((s, i) => finding("src/app.py", null, s, "bug", `Finding ${i + 1}`, "Details.")),
});
const reviews: Record<number, any> = { 1130: review, 295: small(295, ["minor", "minor"]), 412: small(412, ["major", "nit"]) };
const handlers: Record<string, (a: any) => any> = {
  get_settings: () => settings,
  save_settings: () => null,
  list_my_mrs: () => new Promise((r) => setTimeout(() => r({ me: user("me", "Me"), mrs }), 300)),
  mr_details: (a) => ({ ...mrs.find((m) => m.iid === a.iid), description: "Limits failed logins to 5 per 15 minutes per user name.\n\n- `RateLimiter` with a sliding window\n- 429 handling in the web client", state: "opened", detailed_merge_status: "mergeable", head_pipeline: { status: "failed", web_url: "" }, diff_refs: { base_sha: "0", head_sha: "abc123", start_sha: "0" }, source_project_id: a.projectId }),
  mr_diffs: () => new Promise((r) => setTimeout(() => r(fileDiffs), 200)),
  mr_commits: () => [{ id: "abc123def", short_id: "abc123d", title: "Add login rate limiter", author_name: "Alice Developer", authored_date: new Date().toISOString(), web_url: "" }, { id: "fed321", short_id: "fed321c", title: "Handle 429 in the web client and add limiter tests", author_name: "Alice Developer", authored_date: new Date().toISOString(), web_url: "" }],
  commit_diff: () => fileDiffs.slice(0, 1),
  mr_discussions: () => [
    { id: "d1", individual_note: false, notes: [{ id: 1, body: "Mutable default arg!", author: user("bob", "Bob Builder"), created_at: new Date().toISOString(), system: false, resolvable: true, resolved: false, position: { new_path: "src/auth/login.py", old_path: "src/auth/login.py", new_line: 8, old_line: null } }, { id: 2, body: "Will fix", author: user("alice.dev", "Alice Developer"), created_at: new Date().toISOString(), system: false, resolvable: true, resolved: false, position: null }] },
    { id: "d0", individual_note: true, notes: [{ id: 9, body: "## SonarQube Cloud Code Analysis\n## Quality Gate passed\n**Issues**\n![](https://sonarsource.github.io/sonarcloud-github-static-resources/v2/common/passed.svg '') [0 New issues](https://sonarcloud.io/project/issues?id=x)\n\n**Measures**\n![](https://sonarsource.github.io/x/accepted.svg '') [0 Accepted issues](https://sonarcloud.io/a)\n![](https://sonarsource.github.io/x/passed.svg '') [100.0% Coverage on New Code](https://sonarcloud.io/m)\n![](https://sonarsource.github.io/x/failed.svg '') [2 Security Hotspots](https://sonarcloud.io/h)\n\n:white_check_mark: Looks good :tada: :thumbsup: 🚀\n\n| Check | Result |\n|---|---|\n| Lint | **ok** |\n| Tests | ~~flaky~~ fixed |\n\n- [x] migrated\n- [ ] docs\n  - nested `item`\n1. first\n2. second\n\n> quoted *text* with a [link](https://example.com)\n\n```python\nprint(\"<b>not bold</b>\")\n```\n<script>alert(1)</script> <img src=x onerror=alert(2)> [evil](javascript:alert(3)) <details><summary>More</summary>hidden text</details>", author: user("group_1001_bot_0123456789abcdef", "quality-bot"), created_at: new Date().toISOString(), system: false, resolvable: false, resolved: false, position: null }] },
    { id: "d2", individual_note: true, notes: [{ id: 3, body: "LGTM <b>not html</b>", author: user("carol", "Carol"), created_at: new Date().toISOString(), system: false, resolvable: true, resolved: true, position: null }] },
  ],
  open_url: () => null,
  detect_agents: () => [
    { kind: "claude", name: "Claude Code", path: "/Users/you/.local/bin/claude", version: "2.3.1", logged_in: null, models: [] },
    { kind: "codex", name: "Codex", path: "/opt/homebrew/bin/codex", version: "codex-cli 0.170.0", logged_in: new URLSearchParams(location.search).has("codexlogin"), models: [
      { slug: "gpt-6.2-sol", name: "GPT-6.2-Sol", description: "Fast, go-to model for coding and everyday work.", efforts: ["low", "medium", "high", "xhigh", "max"], default_effort: "low" },
      { slug: "gpt-6-luna", name: "GPT-6-Luna", description: "Fast and affordable model for easier tasks.", efforts: ["low", "medium", "high"], default_effort: "medium" },
    ] },
  ],
  load_review: (a) => reviews[a.iid] ?? null,
  get_usage: () => usage,
  cancel_review: () => null,
  delete_review: () => null,
  repo_cache_info: () => ({ count: 3, bytes: 412_000_000 }),
  clear_repo_cache: () => null,
  app_version: () => "0.3.1",
  check_update: () => new Promise((r) => setTimeout(() => r({ current: "0.1.0", version: "0.2.0", date: "2026-12-01", notes: "## What's new\n\n### Features\n- Split diff view\n- Faster repo snapshots\n\n### Fixes\n- Pinned header overlapped the next file", can_install: true, release_url: "https://github.com/dvxzr/Peerlens/releases/tag/v0.2.0" }), 400)),
  install_update: () => new Promise(() => {}),
  default_guidelines: () => "You are a principal engineer reviewing a merge request.\n\nWhat to check, in priority order:\n1. Correctness…\n\nRules:\n- Report only problems introduced by this change…",
  start_review: () => new Promise(() => {}),
  discuss_finding: async (a) => {
    const reply = "Good point. If the list is always capped at 20 items this is a **nit**, not a bug.";
    const activity = a.repoContext ? ["Grep def check  in .", "Read src/auth/utils.py"] : [];
    if (a.repoContext) {
      a.onEvent.onmessage({ type: "repo", stage: "downloading" });
      await new Promise((r) => setTimeout(r, 800));
      a.onEvent.onmessage({ type: "repo", stage: "ready" });
      for (const text of activity) { await new Promise((r) => setTimeout(r, 300)); a.onEvent.onmessage({ type: "activity", text }); }
    }
    for (const w of reply.split(/(?= )/)) { await new Promise((r) => setTimeout(r, 60)); a.onEvent.onmessage({ type: "text", text: w }); }
    const t = review.threads[a.index] ?? [];
    t.push({ role: "user", text: a.message, at: Date.now() / 1000, by: "" }, { role: "agent", text: reply, at: Date.now() / 1000, by: "Claude / sonnet", activity });
    review.threads[a.index] = t;
    a.onEvent.onmessage({ type: "done", thread: t });
  },
  delete_thread: (a) => { delete review.threads[a.index]; return null; },
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
