# Security

Peerlens reads merge requests that may contain hostile content (code, descriptions, comments) and hands them to an AI
agent. The design goal is that **nothing can be written anywhere**: not to GitLab, not to your repositories, not to
your disk outside Peerlens' own data folder.

## Reporting a vulnerability

Please report security issues privately through GitHub's **"Report a vulnerability"** button (Security tab →
Advisories) instead of opening a public issue. Include steps to reproduce and the Peerlens, OS and agent CLI versions.

## What is enforced, and where

| Boundary | Enforcement | Test |
|---|---|---|
| GitLab is read-only | Every request goes through `gitlab/runner.rs`, which always passes `glab api --method GET` and rejects endpoints that look like flags, contain `..`/whitespace, or target GraphQL. No body flags (`-f/-F/--input/--form`) exist in the code. | `gitlab::runner` tests, `tests/security.rs` (source scan) |
| No other processes | Only `gitlab/runner.rs`, `agents/runner.rs` and `platform.rs` (login-shell lookup, URL opener) may start processes; `git` is never run. | `tests/security.rs` |
| Claude Code is read-only | `claude -p --tools "" --disallowedTools … --permission-mode dontAsk --permission-prompts none --safe-mode --restricted --strict-mcp-config` in an empty temp dir. With repo context: only `Read,Grep,Glob`, confined by `--restricted` to the snapshot passed with `--add-dir`. | `agents::runner` argv tests, e2e tests |
| Codex is read-only | `codex exec --sandbox read-only --ignore-user-config --ignore-rules` with the shell, plugins, browser, hooks and sub-agents disabled (diff-only). With repo context the shell stays enabled inside the read-only, offline sandbox. **Codex can read files outside the snapshot**; the prompt forbids it. | argv tests, e2e tests, ignored live test |
| No credentials for the agent | `GITLAB_* GLAB_* GITHUB_* GH_* GIT_* CI_*`, `SSH_AUTH_SOCK` and similar are removed; Git/glab/gh config dirs point to an empty folder; Codex commands see an empty `HOME`. | e2e tests |
| Repo snapshot | Downloaded with `glab api … /repository/archive.tar.gz` (GET), unpacked in-process: regular files only (no symlinks/hardlinks), no `..`/absolute paths, secret-looking files skipped, size limits; deleted after the review. | `review::repo` tests |
| Untrusted text in the UI | MR text and agent output are rendered as text. Markdown is parsed into data and rendered with text nodes (no `{@html}`); only `http(s)` links, opened in the system browser; remote images are never loaded. The only `{@html}` uses are highlight.js output (escaped, structure-checked) and the app's own SVG icons. | WebKit checks in development |
| Webview | Strict CSP (no remote scripts, images or connections), no `withGlobalTauri`, navigation outside the app is blocked, settings from the UI are validated (binary overrides must be absolute paths to `glab*`/`claude*`/`codex*`). | `settings` tests |
| Prompt injection | The system prompt marks all MR content as data. Findings are text only; nothing is applied, committed or posted. | — |

## Known limitations

- Whatever the agent reads (the diff, and with repo context any file in the snapshot) is sent to the agent's provider
  (Anthropic or OpenAI). Secret files are skipped by name only.
- Codex with repo context can technically read any file your user can read (its sandbox is read-only and offline,
  but not limited to one folder).
- macOS release builds are not code-signed or notarized yet. Linux packages are not signed either; verify downloads
  against the release page.
