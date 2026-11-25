<script lang="ts">
  import { kb } from "../lib/platform";
  import type { AgentInfo, Commit, Discussion, FileDiff, Marker, MrDetails, MrSummary, Settings } from "../lib/types";
  import { api, errText } from "../lib/api";
  import { ensureLoaded, get } from "../lib/review.svelte";
  import { ago } from "../lib/time";
  import { fade } from "svelte/transition";
  import { tick, untrack } from "svelte";
  import DiffView from "./DiffView.svelte";
  import Commits from "./Commits.svelte";
  import Discussions from "./Discussions.svelte";
  import ReviewPanel from "./ReviewPanel.svelte";
  import Icon from "./Icon.svelte";
  import Markdown from "./Markdown.svelte";
  import { pref, setPref } from "../lib/prefs";

  let { mr, settings, agents, sidebarOpen, onshowsidebar }: {
    mr: MrSummary; settings: Settings; agents: AgentInfo[]; sidebarOpen: boolean; onshowsidebar: () => void;
  } = $props();
  let collapseLines = $derived(settings.collapse_lines);

  let tab = $state<"changes" | "commits" | "discussions">("changes");
  let details = $state<MrDetails | null>(null);
  let files = $state<FileDiff[] | null>(null);
  let commits = $state<Commit[] | null>(null);
  let discussions = $state<Discussion[] | null>(null);
  let err = $state("");
  let diffView: DiffView | undefined = $state();
  let panel: ReviewPanel | undefined = $state();
  let currentFile = $state<string | null>(null);
  let panelW = $state(pref("panelW", 380));
  let panelOpen = $state(pref("panelOpen", true));
  let filesOpen = $state(pref("filesOpen", true));
  let split = $state(pref("diffSplit", false)); // side-by-side diff
  $effect(() => setPref("diffSplit", split));
  $effect(() => setPref("panelW", panelW));
  $effect(() => setPref("panelOpen", panelOpen));
  $effect(() => setPref("filesOpen", filesOpen));
  let outerW = $state(1200);
  // Keep at least ~480px for the diff: the panel shrinks first when the window is narrow.
  let effPanelW = $derived(Math.round(Math.max(340, Math.min(panelW, outerW - 480 - (filesOpen ? 220 : 0)))));
  let gen = 0; // ignores responses for a previously selected MR

  const id = () => ({ projectId: mr.project_id, iid: mr.iid });

  async function load<T>(f: () => Promise<T>, set: (v: T) => void) {
    const g = gen;
    try {
      const v = await f();
      if (g === gen) set(v);
    } catch (e) {
      if (g === gen) err = errText(e);
    }
  }

  function reload() {
    gen++;
    details = files = commits = discussions = null;
    err = "";
    load(() => api.details(id()), (v) => (details = v));
    load(() => api.diffs(id()), (v) => (files = v));
    if (tab !== "changes") loadTab();
  }
  function loadTab() {
    if (tab === "commits" && !commits) load(() => api.commits(id()), (v) => (commits = v));
    if (tab === "discussions" && !discussions) load(() => api.discussions(id()), (v) => (discussions = v));
  }

  // Re-run ONLY when the selected MR changes. untrack: reload() reads tab/commits and
  // ensureLoaded() reads the review store — tracking those caused an endless reload loop.
  $effect(() => {
    const pid = mr.project_id, iid = mr.iid;
    untrack(() => {
      currentFile = null;
      reload();
      ensureLoaded(pid, iid);
    });
  });
  // Commits and discussions load only when their tab is opened.
  $effect(() => {
    tab;
    untrack(loadTab);
  });

  let markers = $derived.by(() => {
    const m = new Map<string, Marker[]>();
    (get(mr.project_id, mr.iid)?.findings ?? []).forEach((f, id) => {
      if (!f.anchored || f.start_line == null) return;
      const k = `${f.file}:${f.start_line}`;
      (m.get(k) ?? m.set(k, []).get(k)!).push({ severity: f.severity, title: f.title, id });
    });
    return m;
  });
  let findingsByFile = $derived.by(() => {
    const m = new Map<string, number>();
    for (const f of get(mr.project_id, mr.iid)?.findings ?? []) m.set(f.file, (m.get(f.file) ?? 0) + 1);
    return m;
  });

  async function showFile(path: string, line: number | null = null) {
    currentFile = path;
    tab = "changes";
    await tick(); // the Changes tab may have just mounted
    diffView?.scrollTo(path, line);
  }

  // While scrolling the diff, highlight the file at the top in the list and keep it visible.
  function followFile(p: string) {
    currentFile = p;
    queueMicrotask(() => document.querySelector(".frow.cur")?.scrollIntoView({ block: "nearest" }));
  }

  function keys(e: KeyboardEvent) {
    if (!(e.metaKey || e.ctrlKey) || e.shiftKey || e.altKey) return;
    const t = ({ "1": "changes", "2": "commits", "3": "discussions" } as const)[e.key as "1" | "2" | "3"];
    if (t) { e.preventDefault(); tab = t; }
    else if (e.key === "j") { e.preventDefault(); panelOpen = !panelOpen; }
  }

  function drag(e: PointerEvent) {
    const startX = e.clientX, startW = effPanelW;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => (panelW = Math.min(900, Math.max(340, startW - (ev.clientX - startX))));
    const up = () => { el.removeEventListener("pointermove", move); el.removeEventListener("pointerup", up); };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  }

  let totals = $derived(files ? files.reduce((a, f) => [a[0] + f.additions, a[1] + f.deletions], [0, 0]) : [0, 0]);
  let unresolved = $derived(discussions?.filter((d) => d.notes.some((n) => n.resolvable && !n.resolved)).length ?? null);
  const base = (p: string) => p.slice(p.lastIndexOf("/") + 1);
  const dir = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "");
</script>

<svelte:window onkeydown={keys} />
<div class="outer" bind:clientWidth={outerW} style:grid-template-columns={panelOpen ? `minmax(0,1fr) 1px ${effPanelW}px` : "minmax(0,1fr)"}>
  <div class="mr">
    <header data-tauri-drag-region class:nosb={!sidebarOpen}>
      <div class="t1" data-tauri-drag-region>
        {#if !sidebarOpen}<button class="ghost icon" onclick={onshowsidebar} title="Show sidebar ({kb('B')})"><Icon name="sidebarLeft" size={16} /></button>{/if}
        <h1 title={details?.title ?? mr.title}>{details?.title ?? mr.title}</h1>
        <button class="ghost" onclick={() => api.openUrl(mr.web_url)} title="Open in browser"><Icon name="external" size={14} />Open</button>
        <button class="ghost icon" onclick={reload} title="Reload this merge request"><Icon name="refresh" size={15} /></button>
        <button class="ghost icon" class:active={panelOpen} onclick={() => (panelOpen = !panelOpen)} title="{panelOpen ? 'Hide' : 'Show'} AI review panel ({kb('J')})"><Icon name="sidebarRight" size={16} /></button>
      </div>
      <div class="t2">
        <span class="mono ref">{mr.references.full}</span>
        <span class="br"><Icon name="branch" size={13} /><span class="mono">{mr.source_branch}</span><Icon name="arrowRight" size={12} /><span class="mono">{mr.target_branch}</span></span>
        <span class="by"><Icon name="user" size={13} />{mr.author.name || mr.author.username}</span>
        <span class="by"><Icon name="clock" size={13} />{ago(mr.updated_at)}</span>
        {#if details}
          <span class="pills" in:fade={{ duration: 150 }}>
            {#if details.draft}<span class="pill draft">Draft</span>{/if}
            <span class="pill">{details.state}</span>
            {#if details.detailed_merge_status}<span class="pill {details.detailed_merge_status}">{details.detailed_merge_status.replaceAll("_", " ")}</span>{/if}
            {#if details.head_pipeline}<span class="pill {details.head_pipeline.status}"><Icon name="pipeline" size={12} />{details.head_pipeline.status}</span>{/if}
          </span>
        {/if}
      </div>
      {#if details?.description}
        <details class="descbox"><summary>Description</summary><div class="desc"><Markdown text={details.description} /></div></details>
      {/if}
      <nav>
        <button class:on={tab === "changes"} onclick={() => (tab = "changes")} title="Changes ({kb('1')})"><Icon name="diff" size={15} />Changes{#if files}<span class="cnt">{files.length}</span>{/if}</button>
        <button class:on={tab === "commits"} onclick={() => (tab = "commits")} title="Commits ({kb('2')})"><Icon name="commit" size={15} />Commits{#if commits}<span class="cnt">{commits.length}</span>{/if}</button>
        <button class:on={tab === "discussions"} onclick={() => (tab = "discussions")} title="Discussions ({kb('3')})"><Icon name="message" size={15} />Discussions{#if unresolved != null}<span class="cnt">{unresolved}</span>{/if}</button>
        {#if files}
          <span class="totals mono"><span class="add">+{totals[0]}</span> <span class="del">−{totals[1]}</span></span>
          {#if tab !== "discussions"}<button class="ghost icon" class:active={split} onclick={() => (split = !split)} title="{split ? 'Unified' : 'Split'} diff view"><Icon name="columns" size={15} /></button>{/if}
          {#if tab === "changes"}<button class="ghost icon" class:active={filesOpen} onclick={() => (filesOpen = !filesOpen)} title="{filesOpen ? 'Hide' : 'Show'} file list"><Icon name="list" size={15} /></button>{/if}
        {/if}
      </nav>
    </header>
    {#if err}<div class="error"><Icon name="alert" size={14} /><span>{err}</span></div>{/if}
    <section>
      {#key tab}
        <div class="tabbody" in:fade={{ duration: 120 }}>
          {#if tab === "changes"}
            {#if !files}
              <div class="loading">{#each [80, 60, 90, 70, 50, 85] as w, i (i)}<div class="skeleton" style="height:14px;width:{w}%"></div>{/each}</div>
            {:else}
              <div class="changes" style:grid-template-columns={filesOpen ? "220px minmax(0, 1fr)" : "minmax(0, 1fr)"}>
                {#if filesOpen}
                <div class="filelist">
                  {#each files as f (f.old_path + "→" + f.new_path)}
                    {@const p = f.new_path || f.old_path}
                    <button class="frow" class:cur={currentFile === p} onclick={() => showFile(p)} title={p}>
                      <span class="fdot" class:new={f.new_file} class:del={f.deleted_file}></span>
                      <span class="fname"><span>{base(p)}</span>{#if dir(p)}<span class="fdir">{dir(p)}</span>{/if}</span>
                      {#if findingsByFile.get(p)}<span class="fcount" title="Review findings">{findingsByFile.get(p)}</span>{/if}
                      <span class="fstat mono"><span class="add">+{f.additions}</span><span class="del">−{f.deletions}</span></span>
                    </button>
                  {/each}
                </div>
                {/if}
                <div class="diff"><DiffView bind:this={diffView} {files} {collapseLines} {split} {markers} onmarker={(id) => panel?.reveal(id)} onfile={followFile} /></div>
              </div>
            {/if}
          {:else if tab === "commits"}
            {#if !commits}<div class="loading">{#each [70, 60, 75] as w, i (i)}<div class="skeleton" style="height:14px;width:{w}%"></div>{/each}</div>
            {:else}<Commits {commits} projectId={mr.project_id} {collapseLines} {split} />{/if}
          {:else}
            {#if !discussions}<div class="loading">{#each [70, 60, 75] as w, i (i)}<div class="skeleton" style="height:14px;width:{w}%"></div>{/each}</div>
            {:else}<Discussions {discussions} onjump={showFile} />{/if}
          {/if}
        </div>
      {/key}
    </section>
  </div>
  {#if panelOpen}
    <div class="resizer" role="separator" aria-orientation="vertical" onpointerdown={drag}></div>
    <ReviewPanel bind:this={panel} projectId={mr.project_id} iid={mr.iid}
      headSha={details?.diff_refs?.head_sha ?? details?.sha ?? ""} {agents} {settings} {currentFile} onjump={showFile} />
  {/if}
</div>

<style>
  /* minmax(0,1fr) row: every pane fits the window height and scrolls inside. */
  .outer { display: grid; grid-template-rows: minmax(0, 1fr); height: 100%; min-width: 0; min-height: 0; }
  .outer > :global(*) { min-height: 0; }
  .resizer { cursor: col-resize; background: var(--border); position: relative; z-index: 5; }
  .resizer::after { content: ""; position: absolute; inset: 0 -4px; }
  .resizer:hover { background: var(--accent); }
  .mr { display: flex; flex-direction: column; height: 100%; min-width: 0; background: var(--bg1); }
  header { padding: 12px 18px 0; border-bottom: 1px solid var(--border); flex: none; }
  :global(html.mac) header.nosb { padding-left: 88px; } /* room for the macOS traffic lights */
  .ghost.active { color: var(--accent); }
  .t1 { display: flex; gap: 4px; align-items: center; }
  h1 { margin: 0; font-size: 17px; font-weight: 650; flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; letter-spacing: -.01em; }
  .t2 { display: flex; gap: 14px; flex-wrap: wrap; align-items: center; margin: 8px 0 4px; color: var(--text2); font-size: 12px; }
  .ref { color: var(--text3); }
  .br, .by { display: inline-flex; align-items: center; gap: 5px; }
  .br :global(svg), .by :global(svg) { color: var(--text3); }
  .pills { display: inline-flex; gap: 5px; flex-wrap: wrap; }
  .descbox { margin: 6px 0 2px; }
  .descbox summary { cursor: pointer; color: var(--text2); font-size: 12px; user-select: none; }
  .desc { max-height: 260px; overflow: auto; padding: 8px 10px; margin-top: 6px; font-size: 12.5px; background: var(--bg2); border-radius: 8px; }
  nav { display: flex; gap: 4px; margin-top: 8px; align-items: center; }
  nav .ghost.icon { margin-left: 4px; }
  nav button { border: none; border-bottom: 2px solid transparent; border-radius: 0; background: none; padding: 8px 10px 9px; color: var(--text2); gap: 7px; }
  nav button:hover:not(:disabled) { background: none; color: var(--text); }
  nav button:active:not(:disabled) { transform: none; }
  nav button.on { border-bottom-color: var(--accent); color: var(--text); font-weight: 600; }
  .cnt { font-size: 11px; background: var(--bg3); color: var(--text2); border-radius: 6px; padding: 0 6px; line-height: 17px; font-weight: 500; }
  .totals { margin-left: auto; font-size: 12px; display: flex; gap: 6px; }
  .add { color: var(--green); } .del { color: var(--red); }
  .error { display: flex; gap: 8px; margin: 10px 18px 0; padding: 8px 10px; border-radius: 9px; color: var(--red); background: color-mix(in srgb, var(--red) 10%, transparent); font-size: 12px; }
  section { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .tabbody { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .loading { padding: 20px 18px; display: flex; flex-direction: column; gap: 12px; }
  .changes { display: grid; grid-template-rows: minmax(0, 1fr); height: 100%; min-height: 0; }
  .changes > * { min-height: 0; }
  .filelist { overflow-y: auto; border-right: 1px solid var(--border); padding: 8px 6px; background: var(--bg1); }
  .frow { display: flex; gap: 8px; width: 100%; border: none; background: none; text-align: left; padding: 5px 8px; border-radius: 7px; justify-content: flex-start; }
  .frow:hover:not(:disabled) { background: var(--bg2); }
  .frow.cur { background: var(--bg3); }
  .fdot { width: 6px; height: 6px; border-radius: 50%; background: var(--yellow); flex: none; }
  .fdot.new { background: var(--green); } .fdot.del { background: var(--red); }
  .fname { flex: 1; min-width: 0; display: flex; flex-direction: column; font-size: 12.5px; line-height: 1.3; }
  .fname > span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fdir { font-size: 10.5px; color: var(--text3); direction: rtl; text-align: left; }
  .fcount { font-size: 10px; font-weight: 700; color: var(--on-accent); background: var(--accent); border-radius: 8px; padding: 0 5px; line-height: 15px; }
  .fstat { display: flex; gap: 4px; font-size: 11px; }
  .diff { min-width: 0; min-height: 0; }
</style>
