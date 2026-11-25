<script lang="ts">
  import { kb } from "../lib/platform";
  import type { AgentInfo, AgentKind, Finding, MenuItem as Item, Settings } from "../lib/types";
  import { start, get, usage, loadUsage, remove, discuss, removeThread } from "../lib/review.svelte";
  import Markdown from "./Markdown.svelte";
  import { api } from "../lib/api";
  import { ago } from "../lib/time";
  import { onMount, tick } from "svelte";
  import { pref, setPref } from "../lib/prefs";
  import { fly, fade, slide } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import Menu from "./Menu.svelte";
  import Icon from "./Icon.svelte";
  import Rich from "./Rich.svelte";
  import { CLAUDE_MODELS, EFFORTS, LANGUAGES, codexEfforts } from "../lib/options";

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

  // Severity filter; reset when a new review no longer has that severity.
  let sev = $state<string | null>(null);
  let grouped = $derived.by(() => {
    const m = new Map<string, { f: Finding; i: number }[]>();
    (r?.findings ?? []).forEach((f, i) => {
      if (!f.anchored || (sev && f.severity !== sev)) return;
      (m.get(f.file) ?? m.set(f.file, []).get(f.file)!).push({ f, i });
    });
    for (const v of m.values()) v.sort((a, b) => (a.f.start_line ?? 0) - (b.f.start_line ?? 0));
    return [...m.entries()];
  });
  let unanchored = $derived((r?.findings ?? []).map((f, i) => ({ f, i })).filter((x) => !x.f.anchored && (!sev || x.f.severity === sev)));
  let counts = $derived(
    Object.entries((r?.findings ?? []).reduce((a, f) => ((a[f.severity] = (a[f.severity] ?? 0) + 1), a), {} as Record<string, number>))
      .sort(([a], [b]) => (sevOrder[a] ?? 9) - (sevOrder[b] ?? 9)),
  );

  $effect(() => {
    if (sev && !counts.some(([s]) => s === sev)) sev = null;
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
  const winLabel = (n: string) => ({ five_hour: "5-hour", seven_day: "Weekly", seven_day_opus: "Weekly Opus" })[n] ?? n.replaceAll("_", " ");
  const resetIn = (ts: number) => {
    const m = Math.max(0, Math.round((ts * 1000 - Date.now()) / 60000));
    return m >= 60 * 24 ? `${Math.round(m / 1440)}d` : m >= 60 ? `${Math.floor(m / 60)}h ${m % 60}m` : `${m}m`;
  };
  // The most-used limit that hasn't reset yet; shown as one line under the Review button.
  let worst = $derived(usage.value?.windows.filter((w) => w.resets_at * 1000 > Date.now()).sort((a, b) => b.utilization - a.utilization)[0]);
  let overLimit = $derived(agent === "claude" && !!usage.value?.windows.some((w) => w.utilization * 100 >= settings.max_usage_pct && w.resets_at * 1000 > Date.now()));

  // Finding discussion threads: one open at a time, sent with the agent/model chosen above.
  let openThread = $state<number | null>(null);
  let draft = $state("");
  let confirmThread = $state<number | null>(null);
  let threadErr = $state("");
  let modelLabel = $derived(modelItems.find((m) => m.value === model)?.label || "default model");
  function toggleThread(i: number) {
    if (openThread !== i) draft = "";
    openThread = openThread === i ? null : i;
    threadErr = "";
  }
  async function send(i: number) {
    const m = draft.trim();
    if (!m || r?.running || r?.chat || overLimit || codexNotLoggedIn) return;
    draft = "";
    if (!(await discuss(projectId, iid, i, m, agent, model, effort))) draft ||= m;
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

  function run() {
    if (r?.running || r?.chat || (scope === "file" && !currentFile) || overLimit || codexNotLoggedIn) return;
    openThread = null; // a new review replaces findings and their threads
    start(projectId, iid, agent, model, effort, scope === "file" ? currentFile : null, language, repoContext, notes.trim());
  }
  function keys(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") { e.preventDefault(); run(); }
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
  let copied = $state<number | null>(null);
  async function copy(text: string, i: number) {
    try {
      await navigator.clipboard.writeText(text);
      copied = i;
      setTimeout(() => copied === i && (copied = null), 1400);
    } catch { /* clipboard unavailable */ }
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
  <div class="card sevb-{f.severity}" id="finding-{i}" in:fly={{ y: 8, duration: 220, easing: cubicOut }}>
    <div class="head">
      <span class="sev sev-{f.severity}">{f.severity}</span>
      <span class="cat">{f.category}</span>
      {#if f.file}
        <button class="loc mono" onclick={() => onjump(f.file, f.anchored ? f.start_line : null)} title="Show in diff">
          {f.file.split("/").pop()}{f.start_line != null ? `:${f.start_line}${f.end_line && f.end_line !== f.start_line ? "–" + f.end_line : ""}` : ""}
        </button>
      {/if}
    </div>
    <div class="ftitle"><Rich text={f.title} /></div>
    <div class="text"><Rich text={f.explanation} /></div>
    {#if f.snippet}<pre class="code bad">{f.snippet}</pre>{/if}
    {#if f.suggestion && (f.suggestion.description || f.suggestion.code)}
      <div class="sugg">
        <div class="sl"><Icon name="sparkles" size={13} />Suggestion</div>
        {#if f.suggestion.description}<div class="text"><Rich text={f.suggestion.description} /></div>{/if}
        {#if f.suggestion.code}
          <div class="codewrap">
            <div class="codebar">
              <span>Suggested code</span>
              <button class="copy" class:done={copied === i} onclick={() => copy(f.suggestion!.code, i)} title="Copy code">
                <Icon name={copied === i ? "check" : "copy"} size={13} />{copied === i ? "Copied" : "Copy"}
              </button>
            </div>
            <pre class="code good">{f.suggestion.code}</pre>
          </div>
        {/if}
      </div>
    {/if}
    {#if r?.createdAt && !r.running}
      {@const thread = r.threads[i] ?? []}
      <div class="cfoot">
        <button class="ghost disc" class:on={openThread === i} onclick={() => toggleThread(i)} aria-expanded={openThread === i}>
          <Icon name="message" size={13} />Discuss
        </button>
        {#if thread.length}<span class="tcount" title="{thread.length} messages"><Icon name="message" size={11} />{thread.length}</span>{/if}
      </div>
      {#if openThread === i}
        <div class="thread" transition:slide={{ duration: 160 }}>
          <div class="ctx">
            <span>Context:</span>
            <span class="pill"><Icon name="target" size={11} />this finding</span>
            {#if f.file}<span class="pill mono">{f.file.split("/").pop()} diff</span>{/if}
            {#if stale}<span class="pill stalep" title="The MR changed since the review; the agent sees the current diff"><Icon name="clock" size={11} />MR changed</span>{/if}
            <span class="sp"></span>
            {#if thread.length}
              <button class="ghost icon tbtn" class:confirm={confirmThread === i} onclick={() => delThread(i)}
                title={confirmThread === i ? "Click again to delete the thread" : "Delete thread"} disabled={!!r.chat}><Icon name="trash" size={13} /></button>
            {/if}
            <button class="ghost icon tbtn" onclick={() => (openThread = null)} title="Collapse"><Icon name="chevronDown" size={13} style="transform:rotate(180deg)" /></button>
          </div>
          {#each thread as m, j (j)}{@render msg(m.role, m.text, m.by, m.at)}{/each}
          {#if r.chat?.index === i}
            {@render msg("user", r.chat.message, "", null)}
            {@render msg("agent", r.chat.reply, "", null, true)}
          {/if}
          {#if r.chatError?.index === i}<div class="warn"><Icon name="alert" size={14} /><span>{r.chatError.message}</span></div>{/if}
          {#if threadErr}<div class="warn"><Icon name="alert" size={14} /><span>{threadErr}</span></div>{/if}
          <div class="tinp">
            <textarea bind:value={draft} oninput={grow} rows="2" maxlength={4000} placeholder="Reply about this finding…" spellcheck="false"
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
          <div class="tnote"><Icon name="eye" size={11} />Agent sees only this finding and the diff of its file. Each reply resends the thread.</div>
        </div>
      {/if}
    {/if}
  </div>
{/snippet}

{#snippet msg(role: string, text: string, by: string, at: number | null, live = false)}
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
        {#if text}<div class="mtext"><Markdown {text} /></div>{:else}<div class="dim thinking">Thinking…</div>{/if}
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
              ? "Repo context on: the agent may read a read-only snapshot of the repo at the MR head (secrets removed, deleted after). Uses more of your limits."
              : "Repo context off: the agent sees only the diff. Click to let it read the repo."}>
            <Icon name="folder" size={13} />Repo
          </button>
        </div>
        {#if notesOpen}
          <div class="notes" role="group" aria-label="Reviewer notes" onfocusout={notesBlur}>
            <textarea bind:this={notesEl} onkeydown={(e) => e.key === "Escape" && (notesOpen = false)} bind:value={notes} oninput={saveNotes} rows="3" maxlength={MAX_NOTES} spellcheck="false"
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
            <button class="primary go" onclick={run} disabled={!!r?.chat || (scope === "file" && !currentFile) || overLimit || codexNotLoggedIn}
              title={overLimit ? `Paused: usage is over ${settings.max_usage_pct}%` : `Run a read-only review (${kb("↵")})`}>
              <Icon name="sparkles" size={15} />{r?.createdAt ? "Re-review" : "Review"}<kbd>{kb("↵")}</kbd>
            </button>
          {/if}
        </div>
        {#if agent === "claude" && worst}
          {@const pct = Math.round(worst.utilization * 100)}
          <div class="uline" title={usage.value?.windows.map((w) => `${winLabel(w.name)}: ${Math.round(w.utilization * 100)}% · resets in ${resetIn(w.resets_at)}`).join("\n")}>
            <Icon name="gauge" size={12} /><span class="dim">{winLabel(worst.name)}</span>
            <span class="bar"><span class="fill" class:hot={pct >= settings.max_usage_pct} class:warm={pct >= settings.max_usage_pct * 0.7} style:width="{Math.min(100, pct)}%"></span></span>
            <span class="mono">{pct}%</span><span class="dim">· {resetIn(worst.resets_at)}</span>
          </div>
        {/if}
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
      {#if r?.running}<div class="indet" transition:fade={{ duration: 150 }}><span></span></div>{/if}
    {/if}
  </div>
  <div class="results">
    {#if r && (r.createdAt || r.running)}
      <div class="meta">
        {#if r.createdAt}<span class="dim mtxt" title={r.meta}>{r.meta} · {ago(r.createdAt)}</span>{/if}
        {#if stale}<span class="pill stale" title="The MR head changed since this review"><Icon name="clock" size={11} />stale</span>{/if}
        {#if r.cancelled}<span class="pill">cancelled</span>{/if}
        {#if !r.running && r.createdAt}
          <button class="ghost del" class:confirm={confirmDelete} onclick={del} title="Delete this saved review">
            <Icon name="trash" size={13} />{confirmDelete ? "Click again to delete" : "Delete"}
          </button>
        {/if}
      </div>
      {#if delErr}<div class="warn"><Icon name="alert" size={14} /><span>{delErr}</span></div>{/if}
      {#if r.repo}
        <div class="repo dim"><Icon name="folder" size={13} />Repo snapshot: {r.repo.files} files, {mb(r.repo.bytes)} MB{r.repo.excluded.length ? ` · ${r.repo.excluded.length} secret file(s) excluded` : ""}</div>
      {/if}
      {#if r.activity.length}
        <details class="activity" open={r.running}>
          <summary><Icon name="eye" size={13} />Agent activity <span class="dim">({r.activity.length})</span></summary>
          <div class="acts">{#each r.activity as a, i (i)}<div class="act mono">{a}</div>{/each}</div>
        </details>
      {/if}
      {#if counts.length}
        <div class="tabs" role="tablist">
          <button class:on={!sev} role="tab" aria-selected={!sev} onclick={() => (sev = null)}>All <span class="dim">{r.findings.length}</span></button>
          {#each counts as [s, n] (s)}
            <button class:on={sev === s} role="tab" aria-selected={sev === s} onclick={() => (sev = s)} title={s}><i class="dot sevd-{s}"></i>{n} <span class="dim">{s}</span></button>
          {/each}
        </div>
      {/if}
      {#each r.errors as e, i (i)}<div class="warn" transition:slide={{ duration: 160 }}><Icon name="alert" size={14} /><span>{e}</span></div>{/each}
      {#each grouped as [file, items] (file)}
        <div class="group"><Icon name="file" size={13} /><span class="mono">{file}</span></div>
        {#each items as { f, i } (i)}{@render card(f, i)}{/each}
      {/each}
      {#if unanchored.length}
        <div class="group"><Icon name="target" size={13} />Not tied to diff lines</div>
        {#each unanchored as { f, i } (i)}{@render card(f, i)}{/each}
      {/if}
      {#if r.summaries.length}
        <div class="group"><Icon name="checkCircle" size={13} />Summary</div>
        {#each r.summaries as s, i (i)}<div class="summary" in:fade={{ duration: 200 }}><Rich text={s} /></div>{/each}
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
  .controls { padding: 10px; border-bottom: 1px solid var(--border); display: flex; flex-direction: column; gap: 8px; position: relative; }
  .run {
    display: flex; flex-direction: column; gap: 9px; padding: 9px 10px 10px; border-radius: 12px;
    background: linear-gradient(135deg, color-mix(in srgb, var(--accent) 14%, transparent), color-mix(in srgb, var(--accent) 4%, transparent));
    border: 1px solid color-mix(in srgb, var(--accent) 28%, transparent);
  }
  /* Summary line: each Menu renders as a flat, clickable word. */
  .cfg { display: flex; flex-wrap: wrap; align-items: center; gap: 2px; min-width: 0; }
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
  .nadd { justify-content: flex-start; gap: 6px; padding: 2px 4px; border: none; background: transparent; color: var(--text3); font-size: 12px; }
  .nadd:hover:not(:disabled) { background: transparent; color: var(--text); }
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
  .uline { display: flex; align-items: center; gap: 6px; padding: 0 2px; font-size: 11px; cursor: default; }
  .uline :global(svg) { color: var(--text3); flex: none; }
  .uline .bar { flex: 1; }
  .custom { display: flex; gap: 6px; }
  .custom input { flex: 1; min-width: 0; height: 30px; }
  .go { flex: 1; height: 34px; font-size: 13.5px; }
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
  .bar { height: 5px; background: var(--bg4); border-radius: 3px; overflow: hidden; }
  .fill { display: block; height: 100%; background: var(--green); border-radius: 3px; transition: width 400ms var(--ease); }
  .fill.warm { background: var(--yellow); } .fill.hot { background: var(--red); }
  .results { overflow-y: auto; flex: 1; padding: 12px 14px 24px; }
  /* One row: the info text truncates so the Delete button never jumps when its label grows. */
  .meta { display: flex; gap: 6px; align-items: center; font-size: 11.5px; margin-bottom: 8px; }
  .meta > :global(*) { flex: none; }
  .meta > .mtxt { flex: 0 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .del { margin-left: auto; padding: 2px 8px; font-size: 11.5px; gap: 5px; color: var(--text3); white-space: nowrap; }
  .del:hover:not(:disabled) { color: var(--red); }
  .del.confirm { color: var(--red); background: color-mix(in srgb, var(--red) 12%, transparent); }
  .tabs { display: flex; gap: 2px; margin: 0 -2px 8px; border-bottom: 1px solid var(--border); overflow-x: auto; }
  .tabs button {
    border: none; border-bottom: 2px solid transparent; border-radius: 0; background: transparent; padding: 5px 8px; gap: 5px;
    font-size: 12px; color: var(--text2); white-space: nowrap;
  }
  .tabs button:hover:not(:disabled) { background: transparent; color: var(--text); }
  .tabs button:active:not(:disabled) { transform: none; }
  .tabs button.on { color: var(--text); border-bottom-color: var(--accent); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--text3); }
  .sevd-critical { background: var(--red); } .sevd-major { background: var(--orange); } .sevd-minor { background: var(--yellow); }
  .group { display: flex; align-items: center; gap: 6px; font-weight: 600; margin: 14px 0 6px; font-size: 12px; color: var(--text2); overflow-wrap: anywhere; }
  .group :global(svg) { color: var(--text3); flex: none; }
  .card {
    background: var(--bg2); border: 1px solid var(--border); border-left-width: 3px; border-radius: 11px; padding: 10px 12px; margin-bottom: 8px;
    transition: box-shadow 300ms var(--ease), border-color 300ms var(--ease);
  }
  .sevb-critical { border-left-color: var(--red); } .sevb-major { border-left-color: var(--orange); }
  .sevb-minor { border-left-color: var(--yellow); } .sevb-nit { border-left-color: var(--text3); }
  .card:global(.flash) { box-shadow: 0 0 0 2px var(--accent); }
  .head { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  .cat { font-size: 11px; color: var(--text3); }
  .loc { margin-left: auto; padding: 1px 7px; border: none; background: var(--bg3); color: var(--accent); font-size: 11px; border-radius: 6px; }
  .ftitle { font-weight: 600; margin: 6px 0 2px; line-height: 1.35; }
  .text { white-space: pre-wrap; overflow-wrap: anywhere; margin: 4px 0; color: var(--text); line-height: 1.5; }
  .sugg { margin-top: 8px; padding-top: 8px; border-top: 1px dashed var(--border2); }
  .sl { display: flex; align-items: center; gap: 5px; font-size: 11px; font-weight: 600; color: var(--accent); text-transform: uppercase; letter-spacing: .04em; }
  .code { background: var(--bg1); padding: 8px 10px; border-radius: 8px; overflow-x: auto; margin: 6px 0; white-space: pre; border: 1px solid var(--border); line-height: 1.5; }
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
  .cfoot { display: flex; align-items: center; gap: 6px; margin-top: 8px; }
  .disc { padding: 2px 8px; gap: 5px; font-size: 11.5px; }
  .disc.on { color: var(--accent); }
  .tcount { display: inline-flex; align-items: center; gap: 4px; padding: 0 7px; border-radius: 999px; background: var(--accent-bg); color: var(--accent); font-size: 11px; }
  .thread { margin-top: 8px; padding-top: 10px; border-top: 1px solid var(--border); display: flex; flex-direction: column; gap: 10px; }
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
  .tnote { display: flex; align-items: center; gap: 5px; font-size: 11px; color: var(--text3); }
  .summary { background: var(--bg2); border: 1px solid var(--border); padding: 10px 12px; border-radius: 11px; white-space: pre-wrap; line-height: 1.5; }
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
