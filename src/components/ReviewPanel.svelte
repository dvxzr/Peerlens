<script lang="ts">
  import { kb } from "../lib/platform";
  import type { AgentInfo, AgentKind, Finding, MenuItem as Item, Settings } from "../lib/types";
  import { start, get, usage, loadUsage, remove, discuss, removeThread, setStatus } from "../lib/review.svelte";
  import Markdown from "./Markdown.svelte";
  import { api } from "../lib/api";
  import { ago } from "../lib/time";
  import { onMount, tick } from "svelte";
  import { pref, setPref } from "../lib/prefs";
  import { fly, fade, slide } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { cubicOut } from "svelte/easing";
  import Menu from "./Menu.svelte";
  import Icon from "./Icon.svelte";
  import Rich from "./Rich.svelte";
  import { CLAUDE_MODELS, EFFORTS, LANGUAGES, codexEfforts } from "../lib/options";
  import { dedent, pasteSqueezed, unresolved } from "../lib/text";

  let { projectId, iid, headSha, agents, settings, currentFile, onjump }: {
    projectId: number; iid: number; headSha: string; agents: AgentInfo[]; settings: Settings;
    currentFile: string | null; onjump: (path: string, line: number | null) => void;
  } = $props();


  // svelte-ignore state_referenced_locally
  let agent = $state<AgentKind>((agents.find((a) => a.kind === settings.default_agent) ?? agents[0])?.kind ?? "claude");
  // svelte-ignore state_referenced_locally
  let model = $state(agent === "claude" ? settings.default_model : settings.default_codex_model ?? "");
  let scope = $state<"mr" | "file">("mr");
  // svelte-ignore state_referenced_locally
  let language = $state(settings.review_language || "English");
  let customModel = $state("");
  let repoContext = $state(false);
  // svelte-ignore state_referenced_locally
  let effort = $state(settings.default_effort ?? "");

  let agentItems = $derived<Item<AgentKind>[]>(
    agents.map((a) => ({
      value: a.kind, label: a.name, icon: a.kind, color: a.kind === "claude" ? "var(--orange)" : "var(--text)",
      desc: [a.version, a.logged_in === false ? "not logged in" : "", a.path].filter(Boolean).join(" · "),
    })),
  );
  let info = $derived(agents.find((a) => a.kind === agent));
  let codexModel = $derived(info?.models.find((m) => m.slug === model) ?? info?.models[0]);
  let codexNotLoggedIn = $derived(agent === "codex" && info?.logged_in === false);
  let modelItems = $derived.by(() => {
    const base: Item<string>[] = agent === "claude" ? CLAUDE_MODELS : [
      { value: "", label: "Codex default", desc: "Codex picks its default model", icon: "cube" },
      ...(info?.models ?? []).map((m) => ({ value: m.slug, label: m.name, desc: m.description, icon: "cube" as const })),
    ];
    return base.some((m) => m.value === model) ? base : [...base, { value: model, label: model, desc: "Custom model", icon: "cube" as const }];
  });
  let langItems = $derived<Item<string>[]>(
    [...new Set([...LANGUAGES, language, settings.review_language].filter(Boolean))].map((l) => ({ value: l, label: l, icon: "globe" })),
  );
  let scopeItems = $derived<Item<"mr" | "file">[]>([
    { value: "mr", label: "Whole MR", desc: "All reviewable files", icon: "mr" },
    { value: "file", label: currentFile ? `File: ${currentFile.split("/").pop()}` : "Current file", desc: currentFile ? currentFile : "Click a file in Changes first", icon: "file", disabled: !currentFile },
  ]);

  function pickAgent(a: AgentKind) {
    agent = a;
    model = a === "claude" ? settings.default_model : settings.default_codex_model ?? "";
    effort = a === "claude" ? settings.default_effort ?? "" : "";
  }
  // Codex: effort levels depend on the model; drop a level the new model doesn't support.
  let effortItems = $derived(agent === "claude" ? EFFORTS : codexEfforts(codexModel?.efforts ?? ["low", "medium", "high"], codexModel?.default_effort ?? ""));
  $effect(() => {
    if (effort && !effortItems.some((x) => x.value === effort)) effort = "";
  });

  let r = $derived(get(projectId, iid));
  let stale = $derived(!!r && !r.running && !!r.headSha && !!headSha && r.headSha !== headSha);
  const sevOrder: Record<string, number> = { critical: 0, major: 1, minor: 2, nit: 3 };

  // Severity filter ("resolved" = fixed or dismissed); reset when a new review no longer has that kind.
  let sev = $state<string | null>(null);
  const shown = (f: Finding) => !sev || (sev === "resolved" ? !unresolved(f) : unresolved(f) && f.severity === sev);
  let grouped = $derived.by(() => {
    const m = new Map<string, { f: Finding; i: number }[]>();
    (r?.findings ?? []).forEach((f, i) => {
      if (!f.anchored || !shown(f)) return;
      (m.get(f.file) ?? m.set(f.file, []).get(f.file)!).push({ f, i });
    });
    for (const v of m.values()) v.sort((a, b) => (a.f.start_line ?? 0) - (b.f.start_line ?? 0));
    return [...m.entries()];
  });
  let unanchored = $derived((r?.findings ?? []).map((f, i) => ({ f, i })).filter((x) => !x.f.anchored && shown(x.f)));
  let open = $derived((r?.findings ?? []).filter(unresolved));
  let resolvedN = $derived((r?.findings.length ?? 0) - open.length);
  // Severity counters count only findings still to act on.
  let counts = $derived(
    Object.entries(open.reduce((a, f) => ((a[f.severity] = (a[f.severity] ?? 0) + 1), a), {} as Record<string, number>))
      .sort(([a], [b]) => (sevOrder[a] ?? 9) - (sevOrder[b] ?? 9)),
  );

  $effect(() => {
    if (sev && !(sev === "resolved" ? resolvedN : counts.some(([s]) => s === sev))) sev = null;
  });
  const SEV_COLOR: Record<string, string> = { critical: "var(--red)", major: "var(--orange)", minor: "var(--yellow)" };
  // Headline over the summary, derived from the worst finding.
  let verdict = $derived.by((): { label: string; color: string; icon: "alert" | "checkCircle" | "sparkles" } => {
    if (r?.running) return { label: "Summary so far", color: "var(--accent)", icon: "sparkles" };
    const w = counts[0]?.[0];
    if (w === "critical") return { label: "Changes required", color: SEV_COLOR.critical, icon: "alert" };
    if (w === "major") return { label: "Changes recommended", color: SEV_COLOR.major, icon: "alert" };
    if (w) return { label: "Minor changes", color: SEV_COLOR.minor, icon: "alert" };
    if (resolvedN) return { label: "All findings resolved", color: "var(--green)", icon: "checkCircle" };
    return { label: "Looks good", color: "var(--green)", icon: "checkCircle" };
  });

  // Optional reviewer notes, remembered per MR (the panel is reused when switching MRs).
  const MAX_NOTES = 20000; // keep in sync with review::prompt::MAX_NOTES
  let notesKey = $derived(`notes.${projectId}:${iid}`);
  let notes = $state("");
  let notesOpen = $state(false);
  let notesEl: HTMLTextAreaElement | undefined = $state();
  // Grow with the text up to the CSS max-height, then scroll; no manual resize handle.
  $effect(() => {
    void notes;
    if (!notesEl) return;
    notesEl.style.height = "auto";
    notesEl.style.height = `${notesEl.scrollHeight + 2}px`;
  });
  $effect(() => {
    notes = pref<string>(notesKey, "") ?? "";
    notesOpen = false;
    openThread = null;
    draft = "";
  });
  const saveNotes = () => setPref(notesKey, notes);
  async function editNotes() {
    notesOpen = true;
    await tick();
    notesEl?.focus();
  }
  function notesBlur(e: FocusEvent) {
    if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node | null)) notesOpen = false;
  }

  onMount(loadUsage);
  let overLimit = $derived(agent === "claude" && !!usage.value?.windows.some((w) => w.utilization * 100 >= settings.max_usage_pct && w.resets_at * 1000 > Date.now()));

  // Finding discussion threads: one open at a time, sent with the agent/model chosen above.
  let openThread = $state<number | null>(null);
  let draft = $state("");
  let confirmThread = $state<number | null>(null);
  let threadErr = $state("");
  // Discussion reads the repo snapshot of the reviewed commit; on by default when the review used it.
  let threadRepo = $state(false);
  let modelLabel = $derived(modelItems.find((m) => m.value === model)?.label || "default model");
  function toggleThread(i: number) {
    if (openThread !== i) {
      draft = "";
      threadRepo = !!r?.repo;
    }
    openThread = openThread === i ? null : i;
    threadErr = "";
  }
  async function send(i: number) {
    const m = draft.trim();
    if (!m || r?.running || r?.chat || overLimit || codexNotLoggedIn) return;
    draft = "";
    if (!(await discuss(projectId, iid, i, m, agent, model, effort, threadRepo))) draft ||= m;
  }
  async function delThread(i: number) {
    if (confirmThread !== i) {
      confirmThread = i;
      setTimeout(() => confirmThread === i && (confirmThread = null), 3000);
      return;
    }
    confirmThread = null;
    try { await removeThread(projectId, iid, i); threadErr = ""; } catch (e) { threadErr = String(e); }
  }
  function grow(e: Event) {
    const t = e.currentTarget as HTMLTextAreaElement;
    t.style.height = "auto";
    t.style.height = `${t.scrollHeight + 2}px`;
  }

  // The MR got new commits after a whole-MR review: review only those and keep the earlier findings.
  let canIncrement = $derived(stale && r?.scope === "whole MR" && scope === "mr");
  function run(full = false) {
    if (r?.running || r?.chat || (scope === "file" && !currentFile) || overLimit || codexNotLoggedIn) return;
    const since = canIncrement && !full ? r!.headSha : null;
    openThread = null; // a full review replaces findings and their threads
    start(projectId, iid, agent, model, effort, scope === "file" ? currentFile : null, language, repoContext, notes.trim(), since);
  }
  function keys(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") { e.preventDefault(); run(); }
  }
  let statusErr = $state("");
  async function mark(i: number, status: "fixed" | "dismissed") {
    try {
      await setStatus(projectId, iid, i, r?.findings[i]?.status === status ? "" : status);
      statusErr = "";
    } catch (e) { statusErr = String(e); }
  }
  const mb = (b: number) => (b / 1_048_576).toFixed(1);
  let confirmDelete = $state(false);
  let delErr = $state("");
  async function del() {
    if (!confirmDelete) {
      confirmDelete = true;
      setTimeout(() => (confirmDelete = false), 3000);
      return;
    }
    confirmDelete = false;
    try { await remove(projectId, iid); delErr = ""; } catch (e) { delErr = String(e); }
  }
  let copied = $state<string | null>(null); // "code-3" / "card-3"
  async function copy(text: string, key: string) {
    try {
      await navigator.clipboard.writeText(text);
      copied = key;
      setTimeout(() => copied === key && (copied = null), 1400);
    } catch { /* clipboard unavailable */ }
  }
  // A code fence longer than any backtick run inside, so pasted code cannot close it early.
  const fence = (code: string, lang = "") => {
    const ticks = "`".repeat(Math.max(3, ...(code.match(/`+/g) ?? []).map((m) => m.length + 1)));
    return `${ticks}${lang}\n${code.replace(/\n+$/, "")}\n${ticks}`;
  };
  /** The finding (and its discussion) as Markdown to hand to another agent. */
  function findingText(f: Finding, i: number) {
    const lines = f.start_line == null ? "" : `, lines ${f.start_line}${f.end_line && f.end_line !== f.start_line ? `–${f.end_line}` : ""}`;
    const out = [
      `## Code review finding: ${f.title}`,
      `${f.file ? `File: \`${f.file}\`${lines}` : "Not tied to a file"}${r?.headSha ? ` (commit ${r.headSha.slice(0, 8)})` : ""}`,
      `Severity: ${f.severity} · ${f.category}`,
      "",
      f.explanation,
    ];
    if (f.snippet) out.push("", "Problematic code:", fence(f.snippet));
    if (f.suggestion?.description) out.push("", `Suggested fix: ${f.suggestion.description}`);
    if (f.suggestion?.code) out.push("", fence(f.suggestion.code));
    const thread = r?.threads[i] ?? [];
    if (thread.length) {
      out.push("", "Discussion of this finding:");
      for (const m of thread) out.push("", `**${m.role === "user" ? "Developer" : "Reviewer"}:** ${m.text.trim()}`);
    }
    out.push("", "This finding comes from an automated review. Verify it against the current code before changing anything; the suggested fix was not run.");
    return out.join("\n");
  }
  /** Every open finding, in panel order, as one Markdown document. */
  function allText() {
    const items = [...grouped.flatMap(([, v]) => v), ...unanchored].filter(({ f }) => unresolved(f));
    return items.map(({ f, i }) => findingText(f, i)).join("\n\n---\n\n");
  }
  /** Scroll the panel to a finding (used by inline diff markers). */
  export function reveal(i: number) {
    const el = document.getElementById(`finding-${i}`);
    el?.scrollIntoView({ block: "center", behavior: "smooth" });
    el?.classList.add("flash");
    setTimeout(() => el?.classList.remove("flash"), 1100);
  }
</script>

{#snippet card(f: Finding, i: number)}
  {@const done = !unresolved(f)}
  <div class="card" class:done id="finding-{i}" in:fly|global={{ y: 12, duration: r?.running ? 420 : 0, easing: cubicOut }}>
    <div class="head">
      <span class="sev sev-{f.severity}">{f.severity}</span>
      <span class="cat">{f.category}</span>
      {#if f.status === "outdated"}<span class="pill stalep" title="New commits changed this code. Check whether the finding still applies."><Icon name="clock" size={11} />outdated</span>{/if}
      {#if done}<span class="pill okp"><Icon name="check" size={11} />{f.status}</span>{/if}
      {#if f.file}
        <button class="loc mono" onclick={() => onjump(f.file, f.anchored ? f.start_line : null)} title="Show in diff">
          {f.file.split("/").pop()}{f.start_line != null ? `:${f.start_line}${f.end_line && f.end_line !== f.start_line ? "–" + f.end_line : ""}` : ""}
        </button>
      {/if}
      <button class="ghost icon fcopy" class:done={copied === `card-${i}`} onclick={() => copy(findingText(f, i), `card-${i}`)}
        title={copied === `card-${i}` ? "Copied" : "Copy this finding as Markdown for an agent"} aria-label="Copy finding">
        <Icon name={copied === `card-${i}` ? "check" : "copy"} size={13} />
      </button>
    </div>
    <div class="ftitle"><Rich text={f.title} /></div>
    {#if !done}
    <div class="text"><Rich text={f.explanation} /></div>
    {#if f.snippet}<pre class="code bad">{dedent(f.snippet)}</pre>{/if}
    {#if f.suggestion && (f.suggestion.description || f.suggestion.code)}
      <div class="sugg">
        <div class="sl"><Icon name="sparkles" size={13} />Suggestion</div>
        {#if f.suggestion.description}<div class="text"><Rich text={f.suggestion.description} /></div>{/if}
        {#if f.suggestion.code}
          <div class="codewrap">
            <div class="codebar">
              <span>Suggested code</span>
              <button class="copy" class:done={copied === `code-${i}`} onclick={() => copy(f.suggestion!.code, `code-${i}`)} title="Copy code">
                <Icon name={copied === `code-${i}` ? "check" : "copy"} size={13} />{copied === `code-${i}` ? "Copied" : "Copy"}
              </button>
            </div>
            <pre class="code good">{dedent(f.suggestion.code)}</pre>
          </div>
        {/if}
      </div>
    {/if}
    {/if}
    {#if r?.createdAt && !r.running}
      {@const thread = r.threads[i] ?? []}
      <div class="cfoot">
        <button class="ghost disc" class:on={openThread === i} onclick={() => toggleThread(i)} aria-expanded={openThread === i}>
          <Icon name="message" size={13} />Discuss
        </button>
        {#if thread.length}<span class="tcount" title="{thread.length} messages"><Icon name="message" size={11} />{thread.length}</span>{/if}
        <span class="sp"></span>
        <button class="ghost stbtn" class:on={f.status === "fixed"} onclick={() => mark(i, "fixed")} title={f.status === "fixed" ? "Reopen" : "Mark as fixed"}><Icon name="check" size={13} />Fixed</button>
        <button class="ghost stbtn" class:on={f.status === "dismissed"} onclick={() => mark(i, "dismissed")} title={f.status === "dismissed" ? "Reopen" : "Dismiss: not worth fixing or not a real problem"}><Icon name="x" size={13} />Dismiss</button>
      </div>
      {#if openThread === i}
        <div class="thread" transition:slide={{ duration: 260, easing: cubicOut }}>
          <div class="ctx">
            <span>Context:</span>
            <span class="pill"><Icon name="target" size={11} />this finding</span>
            {#if f.file}<span class="pill mono">{f.file.split("/").pop()} diff</span>{/if}
            <button class="rc trc" class:on={threadRepo} onclick={() => (threadRepo = !threadRepo)} role="switch" aria-checked={threadRepo} disabled={!!r.chat}
              title={threadRepo
                ? `The agent may read the repo at the reviewed commit ${r.headSha.slice(0, 8)} (read-only, secrets removed). Downloaded once, then cached.`
                : "Diff only. Click to let the agent read the repo at the reviewed commit."}>
              <Icon name="folder" size={11} />Repo{#if threadRepo}<span class="mono">· {r.headSha.slice(0, 7)}</span>{/if}
            </button>
            {#if stale}<span class="pill stalep" title="The MR changed since the review. The diff is the current one; the repo is the reviewed commit."><Icon name="clock" size={11} />MR changed</span>{/if}
            <span class="sp"></span>
            {#if thread.length}
              <button class="ghost icon tbtn" class:confirm={confirmThread === i} onclick={() => delThread(i)}
                title={confirmThread === i ? "Click again to delete the thread" : "Delete thread"} disabled={!!r.chat}><Icon name="trash" size={13} /></button>
            {/if}
            <button class="ghost icon tbtn" onclick={() => (openThread = null)} title="Collapse"><Icon name="chevronDown" size={13} style="transform:rotate(180deg)" /></button>
          </div>
          {#each thread as m, j (j)}{@render msg(m.role, m.text, m.by, m.at, false, m.activity)}{/each}
          {#if r.chat?.index === i}
            {@render msg("user", r.chat.message, "", null)}
            {#if r.chat.notice}<div class="note-warn"><Icon name="folder" size={13} /><span>{r.chat.notice}</span></div>{/if}
            {@render msg("agent", r.chat.reply, "", null, true, r.chat.activity, r.chat.repo === "downloading" ? "Preparing repo…" : "")}
          {/if}
          {#if r.chatError?.index === i}<div class="warn"><Icon name="alert" size={14} /><span>{r.chatError.message}</span></div>{/if}
          {#if threadErr}<div class="warn"><Icon name="alert" size={14} /><span>{threadErr}</span></div>{/if}
          <div class="tinp">
            <textarea bind:value={draft} oninput={grow} onpaste={pasteSqueezed} rows="2" maxlength={4000} placeholder="Reply about this finding…" spellcheck="false"
              onkeydown={(e) => { if ((e.metaKey || e.ctrlKey) && e.key === "Enter") { e.preventDefault(); e.stopPropagation(); send(i); } }}></textarea>
            <div class="tbar">
              <span class="dim">{agent === "claude" ? "Claude" : "Codex"} · {modelLabel}</span>
              <span class="sp"></span>
              {#if r.chat?.index === i}
                <button class="tstop" onclick={() => api.cancelReview()}><Icon name="stop" size={12} />Cancel</button>
              {:else}
                <button class="primary tsend" onclick={() => send(i)} disabled={!draft.trim() || !!r.chat || overLimit || codexNotLoggedIn}>
                  Send<kbd>{kb("↵")}</kbd>
                </button>
              {/if}
            </div>
          </div>
          <div class="tnote"><Icon name="eye" size={11} />{threadRepo ? "Agent sees this finding, the diff of its file and the repo at the reviewed commit." : "Agent sees only this finding and the diff of its file."} Each reply resends the thread.</div>
        </div>
      {/if}
    {/if}
  </div>
{/snippet}

{#snippet msg(role: string, text: string, by: string, at: number | null, live = false, acts: string[] = [], status = "")}
  {@const isAgent = role === "agent"}
  {@const kind = isAgent ? (by ? (by.startsWith("Codex") ? "codex" : "claude") : agent) : null}
  <div class="msg">
    <span class="av" class:agentav={isAgent} style:color={kind === "claude" ? "var(--orange)" : undefined}>
      <Icon name={kind ?? "user"} size={12} />
    </span>
    <div class="mbody">
      <div class="who">
        <b>{isAgent ? (by || (agent === "claude" ? "Claude" : "Codex")) : "You"}</b>
        {#if live}<span class="dim">· writing…</span>{:else if at}<span class="dim">· {ago(at * 1000)}</span>{/if}
      </div>
      {#if isAgent}
        {#if acts.length}
          <details class="tacts" open={live && !text}>
            <summary><Icon name="eye" size={11} />Read {acts.length} {acts.length === 1 ? "item" : "items"} in the repo</summary>
            {#each acts as a, k (k)}<div class="act mono">{a}</div>{/each}
          </details>
        {/if}
        {#if text}<div class="mtext"><Markdown {text} /></div>{:else}<div class="dim thinking">{status || "Thinking…"}</div>{/if}
      {:else}
        <div class="mtext utext">{text}</div>
      {/if}
    </div>
  </div>
{/snippet}

<svelte:window onkeydown={keys} />
<div class="panel">
  <div class="controls">
    {#if agents.length === 0}
      <div class="warn"><Icon name="alert" size={14} />No agent found. Install Claude Code or Codex, or set the path in Settings.</div>
    {:else}
      <div class="run">
        <div class="cfg">
          <Menu items={agentItems} value={agent} onselect={pickAgent} title="Agent" width={280} label={agent === "claude" ? "Claude" : "Codex"} />
          <Menu items={modelItems} value={model} onselect={(v) => (model = v)} title="Model" width={290}>
            {#snippet footer(close)}
              <form class="custom" onsubmit={(e) => { e.preventDefault(); if (customModel.trim()) { model = customModel.trim(); customModel = ""; close(); } }}>
                <input bind:value={customModel} placeholder="Custom model id…" spellcheck="false" />
                <button type="submit" class="primary">Use</button>
              </form>
            {/snippet}
          </Menu>
          <Menu items={effortItems} value={effort} onselect={(v) => (effort = v)} title="Reasoning effort" width={270}
            label={effort ? effortItems.find((x) => x.value === effort)?.label : "Auto effort"} />
          <Menu items={scopeItems} value={scope} onselect={(v) => (scope = v)} title="What to review" align="right" width={260} />
          <Menu items={langItems} value={language} onselect={(v) => (language = v)} title="Language of the review text" align="right" />
          <button class="rc" class:on={repoContext} onclick={() => (repoContext = !repoContext)} role="switch" aria-checked={repoContext}
            title={repoContext
              ? "Repo context on: the agent may read a read-only snapshot of the repo at the MR head (secrets removed). The snapshot stays in a local cache so finding discussions can read it too. Uses more of your limits."
              : "Repo context off: the agent sees only the diff. Click to let it read the repo."}>
            <Icon name="folder" size={13} />Repo
          </button>
        </div>
        {#if notesOpen}
          <div class="notes" role="group" aria-label="Reviewer notes" onfocusout={notesBlur}>
            <textarea bind:this={notesEl} onkeydown={(e) => e.key === "Escape" && (notesOpen = false)} bind:value={notes} oninput={saveNotes} onpaste={pasteSqueezed} rows="3" maxlength={MAX_NOTES} spellcheck="false"
              placeholder="Task description, what to focus on, what to skip…"></textarea>
            <div class="nbar">
              <span class="ncount mono">{notes.length} / {MAX_NOTES}</span>
            </div>
            <div class="nhint"><Icon name="message" size={12} />MR title and description are already included.</div>
          </div>
        {:else if notes.trim()}
          <div class="nprev">
            <button class="nquote" onclick={editNotes} title="Edit notes"><span>{notes.trim()}</span></button>
            <button class="ghost icon nclear" onclick={() => { notes = ""; saveNotes(); }} title="Clear notes"><Icon name="x" size={13} /></button>
          </div>
        {:else}
          <button class="nadd" onclick={editNotes}><Icon name="message" size={13} />Add context for the reviewer <span class="dim">(optional)</span></button>
        {/if}
        <div class="act">
          {#if r?.running}
            <button class="stopbtn" onclick={() => api.cancelReview()}><Icon name="stop" size={14} />Cancel</button>
            <span class="prog dim">{r.repoStage === "downloading" ? "Downloading repo…" : r.progress ? `Part ${r.progress.chunk} of ${r.progress.total}` : "Starting…"}</span>
          {:else}
            <button class="primary go" onclick={() => run()} disabled={!!r?.chat || (scope === "file" && !currentFile) || overLimit || codexNotLoggedIn}
              title={overLimit ? `Paused: usage is over ${settings.max_usage_pct}%` : canIncrement ? `Review only the commits pushed since the saved review and keep its findings (${kb("↵")})` : `Run a read-only review (${kb("↵")})`}>
              <Icon name="sparkles" size={15} />{canIncrement ? "Review new commits" : r?.createdAt ? "Re-review" : "Review"}<kbd>{kb("↵")}</kbd>
            </button>
            {#if canIncrement}
              <button class="full" onclick={() => run(true)} disabled={!!r?.chat || overLimit || codexNotLoggedIn} title="Review the whole MR again; replaces the saved findings">Full</button>
            {/if}
          {/if}
        </div>
      </div>
      {#if repoContext && agent === "codex" && !r?.running}
        <div class="note-warn"><Icon name="alert" size={13} /><span>Codex reads repo code with shell commands in a read-only, offline sandbox. Unlike Claude it can't be limited to the snapshot folder; the prompt forbids reading elsewhere.</span></div>
      {/if}
      {#if codexNotLoggedIn && !r?.running}
        <div class="note-warn"><Icon name="terminal" size={13} /><span>Codex is not logged in. Run <code>codex login</code> in a terminal, then reopen Settings and press Save to re-check.</span></div>
      {/if}
      {#if overLimit && !r?.running}
        <div class="note-warn"><Icon name="gauge" size={13} />Paused: Claude usage is over {settings.max_usage_pct}%. Raise the threshold in Settings or wait for the reset.</div>
      {/if}
      {#if r?.running}<div class="indet" transition:fade={{ duration: 220 }}><span></span></div>{/if}
    {/if}
  </div>
  <div class="results">
    {#if r && (r.createdAt || r.running)}
      <div class="meta">
        {#if r.createdAt}<span class="dim mtxt" title={r.meta}>{r.meta} · {ago(r.createdAt)}</span>{/if}
        {#if stale}<span class="pill stale" title="The MR head changed since this review"><Icon name="clock" size={11} />stale</span>{/if}
        {#if r.cancelled}<span class="pill">cancelled</span>{/if}
        {#if !r.running && r.createdAt}
          {#if open.length}
            <button class="ghost icon allcopy" class:done={copied === "all"} onclick={() => copy(allText(), "all")}
              title={copied === "all" ? "Copied" : "Copy all open findings as Markdown for an agent"} aria-label="Copy open findings">
              <Icon name={copied === "all" ? "check" : "copy"} size={13} />
            </button>
          {/if}
          <button class="ghost del" class:confirm={confirmDelete} onclick={del} title="Delete this saved review">
            <Icon name="trash" size={13} />{confirmDelete ? "Click again to delete" : "Delete"}
          </button>
        {/if}
      </div>
      {#if delErr}<div class="warn"><Icon name="alert" size={14} /><span>{delErr}</span></div>{/if}
      {#if statusErr}<div class="warn"><Icon name="alert" size={14} /><span>{statusErr}</span></div>{/if}
      {#if r.repo}
        <div class="repo dim"><Icon name="folder" size={13} />Repo snapshot: {r.repo.files} files, {mb(r.repo.bytes)} MB{r.repo.excluded.length ? ` · ${r.repo.excluded.length} secret file(s) excluded` : ""}</div>
      {/if}
      {#if r.activity.length}
        <details class="activity" open={r.running}>
          <summary><Icon name="eye" size={13} />Agent activity <span class="dim">({r.activity.length})</span></summary>
          <div class="acts">{#each r.activity as a, i (i)}<div class="act mono">{a}</div>{/each}</div>
        </details>
      {/if}
      {#if r.summaries.length}
        <div class="verdict" style:--vc={verdict.color}>
          <div class="vh"><Icon name={verdict.icon} size={14} />{verdict.label}</div>
          {#each r.summaries as s, i (i)}<div class="summary" in:fade={{ duration: 300 }}><Rich text={s} /></div>{/each}
        </div>
      {/if}
      {#if r.findings.length}
        <div class="tiles" role="tablist">
          <button class="tile" class:on={!sev} role="tab" aria-selected={!sev} onclick={() => (sev = null)}><b>{r.findings.length}</b><span>All</span></button>
          {#each counts as [s, n] (s)}
            <button class="tile" class:on={sev === s} role="tab" aria-selected={sev === s} onclick={() => (sev = s)} style:--tc={SEV_COLOR[s] ?? "var(--text2)"}><b>{n}</b><span>{s}</span></button>
          {/each}
          {#if resolvedN}
            <button class="tile" class:on={sev === "resolved"} role="tab" aria-selected={sev === "resolved"} onclick={() => (sev = "resolved")} title="Marked fixed or dismissed" style:--tc="var(--green)"><b>{resolvedN}</b><span>Resolved</span></button>
          {/if}
        </div>
      {/if}
      {#each r.errors as e, i (i)}<div class="warn" transition:slide={{ duration: 260, easing: cubicOut }}><Icon name="alert" size={14} /><span>{e}</span></div>{/each}
      {#each grouped as [file, items] (file)}
        <div class="fgroup" animate:flip={{ duration: 320, easing: cubicOut }} in:fade|global={{ duration: r?.running ? 300 : 0 }}>
        <div class="group"><Icon name="file" size={13} /><span class="mono">{file}</span></div>
        {#each items as { f, i } (i)}<div animate:flip={{ duration: 320, easing: cubicOut }}>{@render card(f, i)}</div>{/each}
        </div>
      {/each}
      {#if unanchored.length}
        <div class="group"><Icon name="target" size={13} />Not tied to diff lines</div>
        {#each unanchored as { f, i } (i)}<div animate:flip={{ duration: 320, easing: cubicOut }}>{@render card(f, i)}</div>{/each}
      {/if}
      {#if r.raw}
        <div class="group"><Icon name="terminal" size={13} />Raw agent output</div>
        <pre class="code">{r.raw}</pre>
      {/if}
      {#if r.skipped.length}
        <details class="skipped"><summary>Skipped {r.skipped.length} file{r.skipped.length === 1 ? "" : "s"}</summary>
          {#each r.skipped as s (s.path)}<div class="sk"><span class="mono">{s.path}</span><span class="dim">{s.reason}</span></div>{/each}
        </details>
      {/if}
      {#if !r.running && !r.findings.length && !r.raw && !r.errors.length && r.createdAt}
        <div class="empty"><Icon name="checkCircle" size={22} />No findings.</div>
      {/if}
    {:else}
      <div class="empty">
        <Icon name="shield" size={26} />
        <div>Read-only review</div>
        <div class="dim small">The agent gets the diff as text, has no tools and runs in an empty folder. Nothing is applied, committed or posted.</div>
      </div>
    {/if}
  </div>
</div>

<style>
  .panel { display: flex; flex-direction: column; height: 100%; min-width: 0; min-height: 0; background: var(--bg0); }
  .controls { flex: none; }
  .results { min-height: 0; }
  .controls { padding: 12px; border-bottom: 1px solid var(--border); display: flex; flex-direction: column; gap: 8px; position: relative; }
  .run { display: flex; flex-direction: column; gap: 8px; }
  /* Summary line: each Menu renders as a flat, clickable word. */
  .cfg { display: flex; flex-wrap: wrap; align-items: center; gap: 2px; min-width: 0; padding: 3px; border-radius: 10px; background: var(--bg2); border: 1px solid var(--border); }
  .cfg :global(.menu .trigger) { background: transparent; border-color: transparent; padding: 2px 6px; gap: 5px; font-size: 12px; color: var(--text2); border-radius: 6px; }
  .cfg :global(.menu .trigger:hover), .cfg :global(.menu .trigger.open) { background: color-mix(in srgb, var(--text) 8%, transparent); color: var(--text); }
  .cfg :global(.menu .trigger.open) { box-shadow: inset 0 0 0 1px var(--accent); }
  .cfg :global(.menu .trigger svg:first-child) { width: 13px; height: 13px; }
  .cfg :global(.menu .trigger .chev) { display: none; }
  .cfg :global(.menu:first-child .trigger .lbl) { color: var(--text); }
  .rc { gap: 5px; padding: 2px 6px; font-size: 12px; border-radius: 6px; border: 1px dashed var(--border2); background: transparent; color: var(--text3); }
  .rc:hover:not(:disabled) { background: color-mix(in srgb, var(--text) 8%, transparent); color: var(--text); }
  .rc.on { border-style: solid; border-color: transparent; background: var(--accent-bg); color: var(--accent); }
  .rc.on:hover:not(:disabled) { background: var(--accent-bg); color: var(--accent); }
  .nadd { justify-content: flex-start; gap: 7px; height: 30px; padding: 0 10px; border: 1px dashed var(--border2); border-radius: 9px; background: transparent; color: var(--text3); font-size: 12px; }
  .nadd:hover:not(:disabled) { background: var(--bg2); color: var(--text); border-color: var(--text3); }
  .nadd:active:not(:disabled) { transform: none; }
  .notes { display: flex; flex-direction: column; gap: 6px; }
  .notes textarea { width: 100%; resize: none; min-height: 64px; max-height: 240px; overflow-y: auto; font-size: 12px; line-height: 1.5; background: var(--bg1); border-color: var(--border2); }
  .notes textarea:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-bg); }
  .nbar { display: flex; flex-wrap: wrap; gap: 5px; align-items: center; }
  .ncount { margin-left: auto; font-size: 10.5px; color: var(--text3); }
  .nhint { display: flex; gap: 5px; align-items: center; font-size: 11px; color: var(--text3); }
  .nprev { display: flex; gap: 4px; align-items: flex-start; }
  .nquote {
    flex: 1; min-width: 0; justify-content: flex-start; text-align: left; white-space: pre-wrap; overflow-wrap: anywhere; font-size: 11.5px; color: var(--text2);
    padding: 5px 8px; border: none; border-left: 2px solid var(--accent); border-radius: 0 6px 6px 0; background: color-mix(in srgb, var(--accent) 6%, transparent);
  }
  /* WebKit ignores line-clamp on <button> itself, so clamp an inner span; max-height is the fallback. */
  .nquote span { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; line-height: 1.45; max-height: 2.9em; }
  .nquote:hover:not(:disabled) { background: color-mix(in srgb, var(--accent) 12%, transparent); }
  .nquote:active:not(:disabled) { transform: none; }
  .nclear { width: 24px; height: 24px; color: var(--text3); }
  .act { display: flex; gap: 6px; align-items: center; }
  .custom { display: flex; gap: 6px; }
  .custom input { flex: 1; min-width: 0; height: 30px; }
  .go {
    flex: 1; height: 34px; font-size: 13.5px; border-radius: 9px;
    background: linear-gradient(180deg, color-mix(in srgb, var(--accent) 88%, white), var(--accent2));
    box-shadow: 0 1px 0 rgba(255, 255, 255, .12) inset; transition: filter var(--t) var(--ease), transform 140ms var(--ease), opacity var(--t);
  }
  .go:hover:not(:disabled) { background: linear-gradient(180deg, color-mix(in srgb, var(--accent) 88%, white), var(--accent2)); filter: brightness(1.08); }
  kbd { font: 500 10.5px var(--mono); opacity: .65; margin-left: 4px; }
  .note-warn { display: flex; gap: 7px; align-items: flex-start; font-size: 11.5px; color: var(--orange); line-height: 1.4; }
  .note-warn :global(svg) { flex: none; margin-top: 1px; }
  .note-warn code { font-family: var(--mono); background: var(--bg3); padding: 0 4px; border-radius: 4px; }
  .stopbtn { flex: 1; height: 34px; color: var(--red); border-color: color-mix(in srgb, var(--red) 35%, transparent); }
  .prog { font-size: 12px; white-space: nowrap; }
  .indet { position: absolute; left: 0; right: 0; bottom: -1px; height: 2px; overflow: hidden; }
  .indet span { position: absolute; inset: 0 auto 0 0; width: 35%; background: var(--accent); border-radius: 2px; animation: slide 1.1s var(--ease) infinite; }
  @keyframes slide { from { transform: translateX(-100%); } to { transform: translateX(300%); } }
  .repo { display: flex; align-items: center; gap: 6px; font-size: 11.5px; margin-bottom: 6px; }
  .activity { margin-bottom: 8px; font-size: 12px; }
  .activity summary { cursor: pointer; display: flex; align-items: center; gap: 6px; color: var(--text2); list-style: none; }
  .activity summary :global(svg) { color: var(--text3); }
  .acts { max-height: 180px; overflow-y: auto; margin-top: 6px; padding: 6px 8px; background: var(--bg2); border-radius: 8px; border: 1px solid var(--border); }
  .act { font-size: 11px; color: var(--text2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; line-height: 1.6; }
  .results { overflow-y: auto; flex: 1; padding: 12px 14px 24px; }
  /* One row: the info text truncates so the Delete button never jumps when its label grows. */
  .meta { display: flex; gap: 6px; align-items: center; font-size: 11.5px; margin-bottom: 8px; }
  .meta > :global(*) { flex: none; }
  .meta > .mtxt { flex: 0 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .allcopy { margin-left: auto; width: 24px; height: 22px; color: var(--text3); }
  .allcopy.done { color: var(--green); }
  .allcopy + .del { margin-left: 0; }
  .full { height: 34px; padding: 0 12px; font-size: 12.5px; border-radius: 9px; }
  .stbtn { padding: 2px 7px; gap: 4px; font-size: 11.5px; color: var(--text3); }
  .stbtn.on { color: var(--green); background: color-mix(in srgb, var(--green) 12%, transparent); }
  .card.done { opacity: .6; transition: opacity var(--t) var(--ease); }
  .card.done:hover, .card.done:focus-within { opacity: 1; }
  .pill.okp { background: color-mix(in srgb, var(--green) 12%, transparent); color: var(--green); }
  .del { margin-left: auto; padding: 2px 8px; font-size: 11.5px; gap: 5px; color: var(--text3); white-space: nowrap; }
  .del:hover:not(:disabled) { color: var(--red); }
  .del.confirm { color: var(--red); background: color-mix(in srgb, var(--red) 12%, transparent); }
  /* Severity counters double as the filter. */
  .tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(62px, 1fr)); gap: 6px; margin: 2px 0 10px; }
  .tile { flex-direction: column; align-items: flex-start; gap: 0; padding: 5px 9px 6px; border-radius: 9px; background: var(--bg2); border: 1px solid var(--border); }
  .tile b { font-size: 15px; font-weight: 700; line-height: 1.3; color: var(--tc, var(--text)); font-variant-numeric: tabular-nums; }
  .tile span { font-size: 10.5px; color: var(--text3); text-transform: capitalize; }
  .tile:hover:not(:disabled) { background: var(--bg3); }
  .tile.on { background: var(--bg3); border-color: color-mix(in srgb, var(--tc, var(--accent)) 45%, transparent); }
  .group { display: flex; align-items: center; gap: 6px; font-weight: 600; margin: 14px 0 6px; font-size: 12px; color: var(--text2); overflow-wrap: anywhere; }
  .group :global(svg) { color: var(--text3); flex: none; }
  .card {
    background: var(--bg2); border: 1px solid var(--border); border-radius: 12px; padding: 11px 12px 0; margin-bottom: 8px;
    transition: box-shadow 300ms var(--ease), border-color 300ms var(--ease);
  }
  .card:not(:has(.cfoot)) { padding-bottom: 12px; } /* no footer while the review is running */
  .card:global(.flash) { box-shadow: 0 0 0 2px var(--accent); }
  .head { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  .cat { font-size: 11px; color: var(--text3); }
  .fcopy { margin-left: auto; width: 24px; height: 22px; color: var(--text3); }
  .loc ~ .fcopy { margin-left: 0; }
  .fcopy.done { color: var(--green); }
  .loc { margin-left: auto; padding: 1px 7px; border: none; background: var(--accent-bg); color: var(--accent); font-size: 11px; border-radius: 6px; }
  .ftitle { font-weight: 650; font-size: 13px; margin: 8px 0 2px; line-height: 1.35; }
  .text { white-space: pre-wrap; overflow-wrap: anywhere; margin: 4px 0; color: var(--text2); line-height: 1.5; }
  .sugg { margin-top: 10px; padding: 8px 10px; border-radius: 9px; background: color-mix(in srgb, var(--accent) 6%, transparent); border: 1px solid color-mix(in srgb, var(--accent) 18%, transparent); }
  .sugg .text { color: var(--text); }
  .sl { display: flex; align-items: center; gap: 5px; font-size: 11px; font-weight: 600; color: var(--accent); text-transform: uppercase; letter-spacing: .04em; }
  .code { background: var(--bg1); padding: 8px 10px; border-radius: 8px; overflow-x: auto; margin: 6px 0; white-space: pre; border: 1px solid var(--border); line-height: 1.5; }
  /* Snippets in cards wrap instead of scrolling sideways. */
  .card .code { white-space: pre-wrap; overflow-wrap: anywhere; overflow-x: visible; }
  .code.bad { border-left: 3px solid var(--red); }
  .code.good { border-left: 3px solid var(--green); margin-top: 0; border-top-left-radius: 0; border-top-right-radius: 0; border-top: none; }
  .codewrap { margin-top: 6px; }
  .codebar {
    display: flex; align-items: center; justify-content: space-between; padding: 3px 4px 3px 10px; font-size: 11px; color: var(--text3);
    background: var(--bg3); border: 1px solid var(--border); border-bottom: none; border-radius: 8px 8px 0 0;
  }
  .copy { font-size: 11px; padding: 2px 8px; gap: 4px; background: transparent; border-color: transparent; }
  .copy:hover:not(:disabled) { background: var(--bg4); }
  .copy.done { color: var(--green); }
  .cfoot { display: flex; align-items: center; gap: 6px; margin: 10px -12px 0; padding: 5px 8px; border-top: 1px solid var(--border); }
  .disc { padding: 2px 8px; gap: 5px; font-size: 11.5px; }
  .disc.on { color: var(--accent); }
  .tcount { display: inline-flex; align-items: center; gap: 4px; padding: 0 7px; border-radius: 999px; background: var(--accent-bg); color: var(--accent); font-size: 11px; }
  .thread { padding: 10px 0 12px; border-top: 1px solid var(--border); display: flex; flex-direction: column; gap: 10px; }
  .ctx { display: flex; flex-wrap: wrap; align-items: center; gap: 5px; font-size: 11px; color: var(--text3); }
  .pill { display: inline-flex; align-items: center; gap: 4px; padding: 1px 7px; border-radius: 6px; background: var(--bg3); color: var(--text2); font-size: 11px; }
  .pill.stalep { background: color-mix(in srgb, var(--yellow) 12%, transparent); color: var(--yellow); }
  .sp { flex: 1; }
  .tbtn { width: 24px; height: 24px; color: var(--text3); }
  .tbtn.confirm { color: var(--red); background: color-mix(in srgb, var(--red) 12%, transparent); }
  .msg { display: flex; gap: 8px; min-width: 0; }
  .av { width: 22px; height: 22px; border-radius: 7px; display: grid; place-items: center; flex: none; background: var(--bg4); color: var(--text2); }
  .av.agentav { background: var(--bg3); }
  .mbody { flex: 1; min-width: 0; }
  .who { font-size: 11px; color: var(--text3); margin-bottom: 2px; }
  .who b { color: var(--text2); font-weight: 600; }
  .mtext { font-size: 12.5px; line-height: 1.5; overflow-wrap: anywhere; }
  .utext { white-space: pre-wrap; }
  .thinking { font-size: 12px; animation: pulse 1.2s var(--ease) infinite alternate; }
  @keyframes pulse { from { opacity: .45; } to { opacity: 1; } }
  .tinp { border: 1px solid var(--border2); background: var(--bg1); border-radius: 9px; padding: 6px 8px; transition: border-color var(--t) var(--ease); }
  .tinp:focus-within { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-bg); }
  .tinp textarea { width: 100%; border: none; background: transparent; padding: 0; resize: none; min-height: 36px; max-height: 180px; overflow-y: auto; font-size: 12.5px; line-height: 1.5; }
  .tinp textarea:focus { background: transparent; }
  .tbar { display: flex; align-items: center; gap: 6px; margin-top: 4px; font-size: 11px; }
  .tsend { padding: 3px 10px; font-size: 12px; }
  .tstop { padding: 3px 10px; font-size: 12px; gap: 5px; color: var(--red); border-color: color-mix(in srgb, var(--red) 35%, transparent); }
  .trc { font-size: 11px; padding: 1px 7px; gap: 4px; }
  .trc .mono { font-size: 10.5px; opacity: .8; }
  .tacts { font-size: 11px; margin-bottom: 4px; }
  .tacts summary { cursor: pointer; display: flex; align-items: center; gap: 5px; color: var(--text3); list-style: none; }
  .tacts .act { padding-left: 16px; }
  .tnote { display: flex; align-items: center; gap: 5px; font-size: 11px; color: var(--text3); }
  .verdict {
    margin-bottom: 10px; padding: 10px 12px; border-radius: 12px; border: 1px solid color-mix(in srgb, var(--vc) 28%, transparent);
    background: linear-gradient(180deg, color-mix(in srgb, var(--vc) 10%, transparent), color-mix(in srgb, var(--vc) 3%, transparent));
  }
  .vh { display: flex; align-items: center; gap: 7px; font-weight: 700; font-size: 13px; }
  .vh :global(svg) { color: var(--vc); }
  .summary { margin-top: 6px; white-space: pre-wrap; line-height: 1.5; color: var(--text2); }
  .warn { display: flex; gap: 8px; align-items: flex-start; padding: 8px 10px; border-radius: 9px; color: var(--red); background: color-mix(in srgb, var(--red) 10%, transparent); font-size: 12px; margin-bottom: 6px; overflow-wrap: anywhere; }
  .warn :global(svg) { flex: none; margin-top: 1px; }
  .skipped { margin-top: 14px; font-size: 12px; }
  .skipped summary { cursor: pointer; color: var(--text2); }
  .sk { display: flex; justify-content: space-between; gap: 8px; padding: 3px 0; font-size: 11.5px; }
  .sk .mono { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .empty { display: flex; flex-direction: column; align-items: center; gap: 8px; text-align: center; padding: 40px 18px; color: var(--text2); }
  .empty :global(svg) { color: var(--text3); }
  .small { font-size: 12px; line-height: 1.5; }
</style>
