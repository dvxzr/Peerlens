<script lang="ts">
  import { onMount } from "svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import SettingsDialog from "./components/SettingsDialog.svelte";
  import MrView from "./components/MrView.svelte";
  import Icon from "./components/Icon.svelte";
  import { api, errText } from "./lib/api";
  import type { AgentInfo, MrSummary, Settings } from "./lib/types";
  import { pref, setPref } from "./lib/prefs";

  let mrs = $state<MrSummary[]>([]);
  let loading = $state(false);
  let error = $state("");
  let refreshedAt = $state<number | null>(null);
  let selected = $state<MrSummary | null>(null);
  let settings = $state<Settings | null>(null);
  let showSettings = $state(false);
  let agents = $state<AgentInfo[]>([]);
  let sidebarW = $state(pref("sidebarW", 300));
  let sidebarOpen = $state(pref("sidebarOpen", true));
  let showAll = $state(pref("showAllMrs", false)); // sidebar scope: mine (assignee/reviewer) or all open
  $effect(() => setPref("sidebarW", sidebarW));
  $effect(() => setPref("sidebarOpen", sidebarOpen));
  const toggleSidebar = () => (sidebarOpen = !sidebarOpen);

  const applyTheme = (t: string) => (document.documentElement.dataset.theme = t || "dark");

  // Last loaded list per scope: switching Mine/All shows it instantly; only Refresh re-fetches.
  const lists = new Map<boolean, { mrs: MrSummary[]; at: number }>();

  async function refresh() {
    loading = true;
    error = "";
    const all = showAll;
    try {
      const l = { mrs: (await api.listMyMrs(all)).mrs, at: Date.now() };
      lists.set(all, l);
      if (all === showAll) ({ mrs, at: refreshedAt } = l);
    } catch (e) {
      error = errText(e);
    } finally {
      loading = false;
    }
  }

  function setScope(all: boolean) {
    showAll = all;
    setPref("showAllMrs", all);
    const l = lists.get(all);
    if (!l) return refresh();
    error = "";
    ({ mrs, at: refreshedAt } = l);
  }

  async function saveSettings(s: Settings) {
    try {
      await api.saveSettings(s);
      settings = s;
      lists.clear(); // host or "All" group may have changed
      applyTheme(s.theme);
      showSettings = false;
      agents = await api.detectAgents(); // paths may have changed
    } catch (e) {
      error = errText(e);
    }
  }

  function drag(e: PointerEvent) {
    const startX = e.clientX, startW = sidebarW;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => (sidebarW = Math.min(520, Math.max(240, startW + ev.clientX - startX)));
    const up = () => { el.removeEventListener("pointermove", move); el.removeEventListener("pointerup", up); };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  }

  function keys(e: KeyboardEvent) {
    if (!(e.metaKey || e.ctrlKey) || e.shiftKey || e.altKey) return;
    if (e.key === "r") { e.preventDefault(); if (!loading) refresh(); }
    else if (e.key === "f") { e.preventDefault(); sidebarOpen = true; queueMicrotask(() => document.getElementById("mr-filter")?.focus()); }
    else if (e.key === ",") { e.preventDefault(); showSettings = true; }
    else if (e.key === "b") { e.preventDefault(); toggleSidebar(); }
  }

  // Load once on launch; after that only on explicit Refresh.
  onMount(async () => {
    settings = await api.getSettings();
    applyTheme(settings.theme);
    refresh();
    agents = await api.detectAgents();
  });
</script>

<svelte:window onkeydown={keys} />
<div class="layout" style:grid-template-columns={sidebarOpen ? `${sidebarW}px 1px minmax(0,1fr)` : "minmax(0,1fr)"}>
  {#if sidebarOpen}
    <Sidebar {mrs} {loading} {error} {refreshedAt} {selected} {showAll} onselect={(m) => (selected = m)} onrefresh={refresh}
      onscope={setScope}
      onsettings={() => (showSettings = true)} onhide={toggleSidebar} />
    <div class="resizer" role="separator" aria-orientation="vertical" onpointerdown={drag}></div>
  {/if}
  <main>
    {#if selected && settings}
      <MrView mr={selected} {settings} {agents} {sidebarOpen} onshowsidebar={toggleSidebar} />
    {:else}
      <div class="empty" data-tauri-drag-region>
        <div class="hero"><Icon name="logo" size={44} /></div>
        <div class="et">Pick a merge request</div>
        <div class="dim">Your open MRs as assignee or reviewer are on the left.</div>
      </div>
    {/if}
  </main>
</div>
{#if showSettings && settings}
  <SettingsDialog {settings} {agents} onsave={saveSettings} onclose={() => (showSettings = false)} onpreview={applyTheme} />
{/if}

<style>
  /* minmax(0,1fr) rows: panes must fit the window (WebKit otherwise grows the row to the content height). */
  .layout { display: grid; grid-template-rows: minmax(0, 1fr); height: 100%; }
  .layout > :global(*) { min-height: 0; }
  .resizer { cursor: col-resize; background: var(--border); position: relative; z-index: 5; }
  .resizer::after { content: ""; position: absolute; inset: 0 -4px; }
  .resizer:hover { background: var(--accent); }
  main { min-width: 0; min-height: 0; overflow: hidden; display: flex; flex-direction: column; background: var(--bg1); }
  .empty { margin: auto; display: flex; flex-direction: column; align-items: center; gap: 8px; height: 100%; width: 100%; justify-content: center; }
  .hero { color: var(--text3); margin-bottom: 6px; }
  .et { font-size: 16px; font-weight: 650; }
</style>
