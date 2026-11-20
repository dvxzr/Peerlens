<script lang="ts">
  import { kb } from "../lib/platform";
  import type { AgentInfo, AgentKind, Finding, MenuItem as Item, Settings } from "../lib/types";
  import { start, get, usage, loadUsage, remove } from "../lib/review.svelte";
  import { api } from "../lib/api";
  import { ago } from "../lib/time";
  import { onMount, tick } from "svelte";
  import { pref, setPref } from "../lib/prefs";
  import { fly, fade, slide, scale } from "svelte/transition";
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
  const MAX_NOTES = 4000; // keep in sync with review::prompt::MAX_NOTES
  let notesKey = $derived(`notes.${projectId}:${iid}`);
  let notes = $state("");
  let notesOpen = $state(false);
  let notesEl: HTMLTextAreaElement | undefined = $state();
  $effect(() => {
    notes = pref<string>(notesKey, "") ?? "";
    notesOpen = false;
  });
  const saveNotes = () => setPref(notesKey, notes);
  async function editNotes() {
    notesOpen = true;
    await tick();
    notesEl?.focus();
  }
  function addNote(line: string) {
    if (notes.includes(line)) return;
    notes = (notes.trimEnd() ? notes.trimEnd() + "\n" : "") + line;
    saveNotes();
    notesEl?.focus();
  }
  function notesBlur(e: FocusEvent) {
    if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node | null)) notesOpen = false;
  }

  let showMore = $state(false);
  let moreEl: HTMLDivElement | undefined = $state();
  $effect(() => {
    if (!showMore) return;
    const h = (e: PointerEvent) => !moreEl?.contains(e.target as Node) && (showMore = false);
    const k = (e: KeyboardEvent) => e.key === "Escape" && (showMore = false);
    window.addEventListener("pointerdown", h, true);
    window.addEventListener("keydown", k);
    return () => { window.removeEventListener("pointerdown", h, true); window.removeEventListener("keydown", k); };
  });

  onMount(loadUsage);
  const winLabel = (n: string) => ({ five_hour: "5-hour", seven_day: "Weekly", seven_day_opus: "Weekly Opus" })[n] ?? n.replaceAll("_", " ");
  const resetIn = (ts: number) => {
    const m = Math.max(0, Math.round((ts * 1000 - Date.now()) / 60000));
    return m >= 60 * 24 ? `${Math.round(m / 1440)}d` : m >= 60 ? `${Math.floor(m / 60)}h ${m % 60}m` : `${m}m`;
  };
  // The most-used limit that hasn't reset yet; shown as one line under the Review button.
  let worst = $derived(usage.value?.windows.filter((w) => w.resets_at * 1000 > Date.now()).sort((a, b) => b.utilization - a.utilization)[0]);
  let overLimit = $derived(agent === "claude" && !!usage.value?.windows.some((w) => w.utilization * 100 >= settings.max_usage_pct && w.resets_at * 1000 > Date.now()));

  function run() {
    if (r?.running || (scope === "file" && !currentFile) || overLimit || codexNotLoggedIn) return;
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
          {#if repoContext}<span class="rc" title="Repo context is on"><Icon name="folder" size={13} /></span>{/if}
        </div>
        {#if notesOpen}
          <div class="notes" role="group" aria-label="Reviewer notes" onfocusout={notesBlur}>
            <textarea bind:this={notesEl} onkeydown={(e) => e.key === "Escape" && (notesOpen = false)} bind:value={notes} oninput={saveNotes} rows="3" maxlength={MAX_NOTES} spellcheck="false"
              placeholder="Task description, what to focus on, what to skip…"></textarea>
            <div class="nbar">
              <button class="nchip" onclick={() => addNote("Focus on security.")}>+ security</button>
              <button class="nchip" onclick={() => addNote("Check that tests cover the change.")}>+ tests</button>
              <button class="nchip" onclick={() => addNote("Skip nits and style.")}>− skip nits</button>
              <span class="ncount mono">{notes.length} / {MAX_NOTES}</span>
            </div>
            <div class="nhint"><Icon name="message" size={12} />MR title and description are already included.</div>
          </div>
        {:else if notes.trim()}
          <div class="nprev">
            <button class="nquote" onclick={editNotes} title="Edit notes">{notes.trim()}</button>
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
            <button class="primary go" onclick={run} disabled={(scope === "file" && !currentFile) || overLimit || codexNotLoggedIn}
              title={overLimit ? `Paused: usage is over ${settings.max_usage_pct}%` : `Run a read-only review (${kb("↵")})`}>
              <Icon name="sparkles" size={15} />{r?.createdAt ? "Re-review" : "Review"}<kbd>{kb("↵")}</kbd>
            </button>
          {/if}
          <div class="more" bind:this={moreEl}>
            <button class="icon gear" class:open={showMore} onclick={() => (showMore = !showMore)} title="More options" aria-expanded={showMore}>
              <Icon name="settings" size={15} />
            </button>
            {#if showMore}
              <div class="morepop" transition:scale={{ start: 0.96, duration: 130, easing: cubicOut }}>
                <button class="toggle" class:on={repoContext} onclick={() => (repoContext = !repoContext)} role="switch" aria-checked={repoContext}>
                  <span class="sw"><span class="knob"></span></span>
                  <span class="tt">
                    <span class="tl"><Icon name="folder" size={14} />Repo context</span>
                    <span class="td">{repoContext
                      ? "Agent may read a snapshot of the repo at the MR head (read-only, secrets removed, deleted after). Uses more of your limits."
                      : "Agent sees only the diff."}</span>
                    {#if repoContext && agent === "codex"}
                      <span class="td warnline">Codex reads code with shell commands in a read-only, offline sandbox. Unlike Claude it can't be limited to the snapshot folder; the prompt forbids reading elsewhere.</span>
                    {/if}
                  </span>
                </button>
                {#if agent === "claude" && usage.value?.windows.length}
                  <div class="usage" title="Your Claude subscription usage, as reported by Claude Code">
                    {#each usage.value.windows as w (w.name)}
                      {@const pct = Math.round(w.utilization * 100)}
                      <div class="uw">
                        <span class="ul">{winLabel(w.name)}</span>
                        <span class="bar"><span class="fill" class:hot={pct >= settings.max_usage_pct} class:warm={pct >= settings.max_usage_pct * 0.7} style:width="{Math.min(100, pct)}%"></span></span>
                        <span class="up mono">{pct}%</span>
                        <span class="ur dim">resets in {resetIn(w.resets_at)}</span>
                      </div>
                    {/each}
                  </div>
                {/if}
              </div>
            {/if}
          </div>
        </div>
        {#if agent === "claude" && worst}
          {@const pct = Math.round(worst.utilization * 100)}
          <button class="uline" onclick={() => (showMore = true)} title="Claude usage: all limits in More options">
            <Icon name="gauge" size={12} /><span class="dim">{winLabel(worst.name)}</span>
            <span class="bar"><span class="fill" class:hot={pct >= settings.max_usage_pct} class:warm={pct >= settings.max_usage_pct * 0.7} style:width="{Math.min(100, pct)}%"></span></span>
            <span class="mono">{pct}%</span><span class="dim">· {resetIn(worst.resets_at)}</span>
          </button>
        {/if}
      </div>
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
        {#if r.createdAt}<span class="dim">{r.meta} · {ago(r.createdAt)}</span>{/if}
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
  .rc { display: inline-flex; padding: 2px 5px; color: var(--accent); }
  .nadd { justify-content: flex-start; gap: 6px; padding: 2px 4px; border: none; background: transparent; color: var(--text3); font-size: 12px; }
  .nadd:hover:not(:disabled) { background: transparent; color: var(--text); }
  .nadd:active:not(:disabled) { transform: none; }
  .notes { display: flex; flex-direction: column; gap: 6px; }
  .notes textarea { width: 100%; resize: vertical; min-height: 64px; max-height: 240px; font-size: 12px; line-height: 1.5; background: var(--bg1); border-color: var(--border2); }
  .notes textarea:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-bg); }
  .nbar { display: flex; flex-wrap: wrap; gap: 5px; align-items: center; }
  .nchip { padding: 1px 8px; border-radius: 999px; font-size: 11px; background: transparent; border-color: var(--border2); color: var(--text2); }
  .nchip:hover:not(:disabled) { background: transparent; border-color: var(--accent); color: var(--accent); }
  .ncount { margin-left: auto; font-size: 10.5px; color: var(--text3); }
  .nhint { display: flex; gap: 5px; align-items: center; font-size: 11px; color: var(--text3); }
  .nprev { display: flex; gap: 4px; align-items: flex-start; }
  .nquote {
    flex: 1; min-width: 0; justify-content: flex-start; text-align: left; white-space: pre-wrap; overflow-wrap: anywhere; font-size: 11.5px; color: var(--text2);
    padding: 5px 8px; border: none; border-left: 2px solid var(--accent); border-radius: 0 6px 6px 0; background: color-mix(in srgb, var(--accent) 6%, transparent);
    display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
  }
  .nquote:hover:not(:disabled) { background: color-mix(in srgb, var(--accent) 12%, transparent); }
  .nquote:active:not(:disabled) { transform: none; }
  .nclear { width: 24px; height: 24px; color: var(--text3); }
  .act { display: flex; gap: 6px; align-items: center; }
  .more { position: relative; flex: none; }
  .gear { width: 34px; height: 34px; padding: 0; }
  .gear.open { border-color: var(--accent); color: var(--accent); background: var(--accent-bg); }
  .morepop {
    position: absolute; top: calc(100% + 6px); right: 0; z-index: 50; width: min(320px, 80vw); transform-origin: top right;
    background: var(--bg2); border-radius: 12px; box-shadow: var(--shadow); padding: 6px; display: flex; flex-direction: column; gap: 6px;
  }
  .morepop .toggle { background: transparent; border-color: transparent; }
  .morepop .toggle:hover { background: var(--bg3); }
  .uline { justify-content: flex-start; gap: 6px; padding: 0 2px; border: none; background: transparent; font-size: 11px; }
  .uline:hover:not(:disabled) { background: transparent; }
  .uline:active:not(:disabled) { transform: none; }
  .uline :global(svg) { color: var(--text3); flex: none; }
  .uline .bar { flex: 1; }
  .custom { display: flex; gap: 6px; }
  .custom input { flex: 1; min-width: 0; height: 30px; }
  .go { flex: 1; height: 34px; font-size: 13.5px; }
  kbd { font: 500 10.5px var(--mono); opacity: .65; margin-left: 4px; }
  .note-warn { display: flex; gap: 7px; align-items: flex-start; font-size: 11.5px; color: var(--orange); line-height: 1.4; }
  .note-warn :global(svg) { flex: none; margin-top: 1px; }
  .note-warn code { font-family: var(--mono); background: var(--bg3); padding: 0 4px; border-radius: 4px; }
  .td.warnline { color: var(--orange); }
  .stopbtn { flex: 1; height: 34px; color: var(--red); border-color: color-mix(in srgb, var(--red) 35%, transparent); }
  .prog { font-size: 12px; white-space: nowrap; }
  .indet { position: absolute; left: 0; right: 0; bottom: -1px; height: 2px; overflow: hidden; }
  .indet span { position: absolute; inset: 0 auto 0 0; width: 35%; background: var(--accent); border-radius: 2px; animation: slide 1.1s var(--ease) infinite; }
  @keyframes slide { from { transform: translateX(-100%); } to { transform: translateX(300%); } }
  .toggle { justify-content: flex-start; align-items: flex-start; gap: 10px; text-align: left; background: var(--bg2); padding: 8px 10px; border-radius: 9px; }
  .toggle:active:not(:disabled) { transform: none; }
  .sw { width: 30px; height: 18px; border-radius: 9px; background: var(--bg4); position: relative; flex: none; margin-top: 1px; transition: background-color var(--t) var(--ease); }
  .knob { position: absolute; top: 2px; left: 2px; width: 14px; height: 14px; border-radius: 50%; background: var(--text2); transition: transform 180ms var(--ease), background-color var(--t); }
  .toggle.on .sw { background: var(--accent); }
  .toggle.on .knob { transform: translateX(12px); background: #fff; }
  .tt { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .tl { display: flex; align-items: center; gap: 6px; font-weight: 600; }
  .tl :global(svg) { color: var(--text3); }
  .td { font-size: 11.5px; color: var(--text3); line-height: 1.4; white-space: normal; }
  .repo { display: flex; align-items: center; gap: 6px; font-size: 11.5px; margin-bottom: 6px; }
  .activity { margin-bottom: 8px; font-size: 12px; }
  .activity summary { cursor: pointer; display: flex; align-items: center; gap: 6px; color: var(--text2); list-style: none; }
  .activity summary :global(svg) { color: var(--text3); }
  .acts { max-height: 180px; overflow-y: auto; margin-top: 6px; padding: 6px 8px; background: var(--bg2); border-radius: 8px; border: 1px solid var(--border); }
  .act { font-size: 11px; color: var(--text2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; line-height: 1.6; }
  .usage { display: flex; flex-direction: column; gap: 5px; padding: 8px 10px; background: var(--bg2); border-radius: 9px; border: 1px solid var(--border); }
  .uw { display: grid; grid-template-columns: 74px 1fr 34px auto; gap: 8px; align-items: center; font-size: 11.5px; }
  .ul { color: var(--text2); }
  .bar { height: 5px; background: var(--bg4); border-radius: 3px; overflow: hidden; }
  .fill { display: block; height: 100%; background: var(--green); border-radius: 3px; transition: width 400ms var(--ease); }
  .fill.warm { background: var(--yellow); } .fill.hot { background: var(--red); }
  .up { text-align: right; font-size: 11px; }
  .ur { font-size: 11px; }
  .results { overflow-y: auto; flex: 1; padding: 12px 14px 24px; }
  .meta { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; font-size: 11.5px; margin-bottom: 8px; }
  .del { margin-left: auto; padding: 2px 8px; font-size: 11.5px; gap: 5px; color: var(--text3); }
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
