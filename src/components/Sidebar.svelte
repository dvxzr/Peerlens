<script lang="ts">
  import { kb } from "../lib/platform";
  import type { MrSummary } from "../lib/types";
  import { ago } from "../lib/time";
  import Icon from "./Icon.svelte";
  import { pref, setPref } from "../lib/prefs";
  import { updates } from "../lib/updates.svelte";

  let { mrs, loading, error, refreshedAt, selected, showAll, onselect, onrefresh, onscope, onsettings, onhide }: {
    mrs: MrSummary[]; loading: boolean; error: string; refreshedAt: number | null; selected: MrSummary | null; showAll: boolean;
    onselect: (m: MrSummary) => void; onrefresh: () => void; onscope: (all: boolean) => void; onsettings: () => void; onhide: () => void;
  } = $props();

  // Re-render relative times ("updated 3 minutes ago") while the list sits idle.
  let now = $state(Date.now());
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(t);
  });

  let q = $state("");
  // Groups are collapsed by default; the ones you open are remembered.
  let expanded = $state(new Set<string>(pref<string[]>("expandedProjects", [])));
  const isOpen = (p: string) => expanded.has(p) || !!q.trim();
  const project = (m: MrSummary) => m.references.full.replace(/!\d+$/, "") || `project ${m.project_id}`;
  const short = (p: string) => p.split("/").slice(-2).join("/");

  let groups = $derived.by(() => {
    const needle = q.trim().toLowerCase();
    const map = new Map<string, MrSummary[]>();
    for (const m of mrs) {
      const p = project(m);
      const hay = `${m.title} !${m.iid} ${m.author.username} ${m.author.name} ${p} ${m.source_branch} ${m.target_branch}`.toLowerCase();
      if (needle && !hay.includes(needle)) continue;
      (map.get(p) ?? map.set(p, []).get(p)!).push(m);
    }
    return [...map.entries()].sort(([a], [b]) => a.localeCompare(b));
  });
  function fold(p: string, open = !expanded.has(p)) {
    if (open === expanded.has(p)) return;
    const s = new Set(expanded);
    open ? s.add(p) : s.delete(p);
    expanded = s;
    setPref("expandedProjects", [...s]);
  }
</script>

<aside>
  <div class="titlebar" data-tauri-drag-region>
    <div class="brand" data-tauri-drag-region><span class="logo"><Icon name="logo" size={20} /></span>Peerlens</div>
    <button class="ghost icon" onclick={onrefresh} disabled={loading} title="Refresh merge requests ({kb('R')})">
      <Icon name="refresh" size={16} class={loading ? "spin" : ""} />
    </button>
    <button class="ghost icon setbtn" onclick={onsettings}
      title={updates.available ? `Update available: v${updates.available.version} (Settings → Updates)` : `Settings (${kb(",")})`}>
      <Icon name="settings" size={16} />{#if updates.available}<span class="udot"></span>{/if}
    </button>
    <button class="ghost icon" onclick={onhide} title="Hide sidebar ({kb('B')})"><Icon name="sidebarLeft" size={16} /></button>
  </div>
  <div class="searchbox">
    <Icon name="search" size={14} class="sicon" />
    <input id="mr-filter" placeholder="Filter by title, !iid, author, branch" bind:value={q} spellcheck="false"
      onkeydown={(e) => e.key === "Escape" && ((q = ""), e.currentTarget.blur())} />
    {#if !q}<kbd class="hint">{kb("F")}</kbd>{/if}
    {#if q}<button class="ghost clear" onclick={() => (q = "")} title="Clear"><Icon name="x" size={13} /></button>{/if}
  </div>
  <div class="status dim">
    <div class="seg" role="group" aria-label="Which merge requests">
      <button class:on={!showAll} disabled={loading} onclick={() => showAll && onscope(false)} title="Assigned to me or I'm a reviewer">Mine</button>
      <button class:on={showAll} disabled={loading} onclick={() => !showAll && onscope(true)} title="Recently updated open merge requests in your groups">All</button>
    </div>
    <span class="stxt">{#if loading}Refreshing…{:else if refreshedAt}{mrs.length} open · updated {ago(refreshedAt, now)}{:else}Not loaded yet{/if}</span>
  </div>
  {#if error}<div class="error"><Icon name="alert" size={14} /><span>{error}</span></div>{/if}
  <div class="list">
    {#if loading && !mrs.length}
      {#each [0, 1, 2, 3, 4] as i (i)}<div class="skel"><div class="skeleton" style="height:12px;width:70%"></div><div class="skeleton" style="height:10px;width:45%;margin-top:8px"></div></div>{/each}
    {/if}
    {#each groups as [proj, items] (proj)}
      <button class="group" onclick={() => fold(proj)} title={proj}>
        <Icon name="chevronRight" size={12} class="gchev {isOpen(proj) ? 'open' : ''}" />
        <Icon name="folder" size={15} class="gfolder" />
        <span class="gname">{short(proj)}</span>
        <span class="count">{items.length}</span>
      </button>
      {#if isOpen(proj)}
        {#each items as m (m.project_id + ":" + m.iid)}
          {@const sel = selected?.project_id === m.project_id && selected?.iid === m.iid}
          <button class="mr" class:sel onclick={() => { fold(proj, true); onselect(m); }}>
            <div class="top">
              <Icon name="mr" size={15} class="micon {m.draft ? 'draft' : ''}" />
              <span class="title">{m.title.replace(/^Draft:\s*/i, "")}</span>
            </div>
            <div class="meta">
              <span class="mono iid">!{m.iid}</span>
              <span class="who">{m.author.username}</span>
              <span class="dot">·</span>
              <span>{ago(m.updated_at, now)}</span>
            </div>
            <div class="tags">
              {#if m.draft}<span class="pill draft">Draft</span>{/if}
              {#if m.is_assignee}<span class="pill">assignee</span>{/if}
              {#if m.is_reviewer}<span class="pill">reviewer</span>{/if}
              <span class="branch mono"><Icon name="arrowRight" size={11} />{m.target_branch}</span>
            </div>
          </button>
        {/each}
      {/if}
    {:else}
      {#if !loading && !error}<div class="empty dim">{mrs.length ? "Nothing matches the filter." : "No open merge requests."}</div>{/if}
    {/each}
  </div>
</aside>

<style>
  aside { display: flex; flex-direction: column; height: 100%; background: var(--bg0); min-width: 0; }
  /* 46px tall: content centre (23px) matches the traffic lights (tauri.conf.json trafficLightPosition y=25). */
  .titlebar { height: 46px; display: flex; align-items: center; gap: 2px; padding: 0 10px 0 16px; flex: none; }
  :global(html.mac) .titlebar { padding-left: 86px; } /* room for the traffic lights */
  .brand { flex: 1; display: flex; align-items: center; gap: 8px; font-weight: 650; font-size: 13.5px; letter-spacing: .01em; }
  .logo { display: grid; place-items: center; color: var(--text); }
  .setbtn { position: relative; }
  .udot { position: absolute; top: 4px; right: 4px; width: 7px; height: 7px; border-radius: 50%; background: var(--accent); box-shadow: 0 0 0 2px var(--bg0); }
  .searchbox { position: relative; margin: 2px 10px 6px; display: flex; align-items: center; }
  .searchbox :global(.sicon) { position: absolute; left: 10px; color: var(--text3); pointer-events: none; }
  .searchbox input { width: 100%; padding-left: 30px; padding-right: 28px; background: var(--bg2); height: 32px; }
  :global(html:not(.mac)) .searchbox input { padding-right: 52px; } /* "Ctrl+F" is wider than "⌘F" */
  .hint { position: absolute; right: 9px; font: 500 10.5px var(--mono); color: var(--text3); pointer-events: none; }
  .clear { position: absolute; right: 3px; padding: 4px; height: 26px; width: 26px; }
  .status { font-size: 11px; padding: 0 10px 6px; display: flex; align-items: center; gap: 8px; }
  .stxt { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .seg { display: inline-flex; flex: none; background: var(--bg2); border: 1px solid var(--border); border-radius: 8px; padding: 2px; }
  .seg button { border: none; background: none; padding: 2px 9px; border-radius: 6px; color: var(--text2); font-size: 11px; }
  .seg button.on { background: var(--bg4); color: var(--text); font-weight: 600; }
  .error { display: flex; gap: 8px; align-items: flex-start; margin: 4px 10px 8px; padding: 8px 10px; border-radius: 9px; color: var(--red); background: color-mix(in srgb, var(--red) 10%, transparent); font-size: 12px; overflow-wrap: anywhere; }
  .list { overflow-y: auto; flex: 1; padding: 0 8px 12px; }
  .group {
    width: 100%; justify-content: flex-start; gap: 7px; background: none; border: none; padding: 9px 6px 5px; border-radius: 7px; color: var(--text2);
  }
  .group:hover:not(:disabled) { background: none; color: var(--text); }
  .group:active:not(:disabled) { transform: none; }
  .group :global(.gchev) { color: var(--text3); transition: transform var(--t) var(--ease); }
  .group :global(.gchev.open) { transform: rotate(90deg); }
  .group :global(.gfolder) { color: var(--text3); }
  .gname { font-weight: 650; font-size: 12.5px; color: var(--text); flex: 1; text-align: left; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .count { font-size: 11px; color: var(--text3); background: var(--bg3); border-radius: 6px; padding: 0 6px; line-height: 18px; min-width: 22px; text-align: center; font-variant-numeric: tabular-nums; }
  .mr {
    display: block; width: 100%; text-align: left; background: transparent; border: 1px solid transparent; border-radius: var(--radius);
    padding: 8px 10px 9px; margin: 1px 0;
  }
  .mr:hover:not(:disabled) { background: var(--bg2); }
  .mr:active:not(:disabled) { transform: scale(.99); }
  .mr.sel { background: var(--bg2); border-color: var(--border2); box-shadow: 0 1px 0 rgba(0,0,0,.15); }
  .top { display: flex; gap: 8px; align-items: flex-start; }
  .top :global(.micon) { color: var(--green); margin-top: 1px; }
  .top :global(.micon.draft) { color: var(--text3); }
  .title { font-weight: 550; font-size: 13px; line-height: 1.35; overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .meta { display: flex; gap: 6px; font-size: 11.5px; color: var(--text3); margin: 4px 0 0 23px; align-items: center; }
  .iid { color: var(--text2); font-size: 11px; }
  .who { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 140px; }
  .tags { display: flex; gap: 5px; flex-wrap: wrap; margin: 6px 0 0 23px; align-items: center; }
  .branch { display: inline-flex; align-items: center; gap: 3px; font-size: 11px; color: var(--text3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 170px; }
  .skel { padding: 12px 10px; }
  .empty { padding: 16px 10px; text-align: center; }
</style>
