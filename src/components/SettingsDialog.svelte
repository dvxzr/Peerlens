<script lang="ts">
  import type { AgentInfo, CacheInfo, MenuItem, Settings } from "../lib/types";
  import { fade, scale } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import Icon from "./Icon.svelte";
  import Menu from "./Menu.svelte";
  import type { IconName } from "../lib/icons";
  import { EFFORTS, LANGUAGES } from "../lib/options";
  import { api } from "../lib/api";
  import { onMount } from "svelte";
  import Markdown from "./Markdown.svelte";
  import { updates, checkForUpdates, installUpdate, loadVersion } from "../lib/updates.svelte";
  import { pref, setPref } from "../lib/prefs";
  import { kb } from "../lib/platform";

  let { settings, agents = [], onsave, onclose, onpreview }: {
    settings: Settings; agents?: AgentInfo[]; onsave: (s: Settings) => void; onclose: () => void; onpreview: (theme: Settings["theme"]) => void;
  } = $props();
  let codex = $derived(agents.find((a) => a.kind === "codex"));
  let codexModelItems = $derived<MenuItem<string>[]>([
    { value: "", label: "Codex default", icon: "cube" },
    ...(codex?.models ?? []).map((m) => ({ value: m.slug, label: m.name, desc: m.description, icon: "cube" as const })),
  ]);
  // svelte-ignore state_referenced_locally
  let s = $state({ ...settings }); // edit a copy; saved only on Save

  type Page = "general" | "review" | "prompt" | "limits" | "repo" | "connections";
  const pages: [Page, string, IconName][] = [
    ["general", "General", "settings"], ["review", "Review", "sparkles"], ["prompt", "Review prompt", "message"],
    ["limits", "Usage limits", "gauge"], ["repo", "Repo context", "folder"], ["connections", "Connections", "terminal"],
  ];
  let page = $state<Page>(pref("settingsPage", "general"));
  $effect(() => setPref("settingsPage", page));

  const themes: [Settings["theme"], string, IconName][] = [["dark", "Dark", "moon"], ["light", "Light", "sun"], ["system", "System", "monitor"]];
  const agentItems: MenuItem<string>[] = [
    { value: "claude", label: "Claude Code", icon: "claude", color: "var(--orange)" },
    { value: "codex", label: "Codex", icon: "codex" },
  ];
  const modelItems: MenuItem<string>[] = [
    { value: "sonnet", label: "Sonnet", desc: "Recommended", icon: "sparkles", color: "var(--orange)" },
    { value: "opus", label: "Opus", desc: "Most thorough, heaviest on limits", icon: "sparkles", color: "var(--purple)" },
    { value: "haiku", label: "Haiku", desc: "Fastest, lightest on limits", icon: "sparkles", color: "var(--green)" },
    { value: "fable", label: "Fable", icon: "sparkles", color: "var(--blue)" },
    { value: "", label: "CLI default", desc: "Whatever your claude config uses", icon: "cube" },
  ];
  // Review prompt: an empty setting means "built-in default"; show the default text so it can be edited.
  let defaultGuide = $state("");
  let guide = $state(s.review_guidelines);
  let cache = $state<CacheInfo | null>(null);
  let cacheErr = $state("");
  async function clearCache() {
    try {
      await api.clearRepoCache();
      cacheErr = "";
    } catch (e) {
      cacheErr = String(e);
    }
    cache = await api.repoCacheInfo();
  }
  onMount(async () => {
    loadVersion();
    api.repoCacheInfo().then((c) => (cache = c));
    defaultGuide = await api.defaultGuidelines();
    if (!guide.trim()) guide = defaultGuide;
  });
  // Until the default text loads, a non-empty prompt can only be a saved custom one.
  let customized = $derived(defaultGuide ? guide.trim() !== defaultGuide.trim() : !!guide.trim());
  let customLang = $state("");
  let langItems = $derived<MenuItem<string>[]>(
    [...new Set([...LANGUAGES, s.review_language].filter(Boolean))].map((l) => ({ value: l, label: l, icon: "globe" })),
  );
  const result = () => ({ ...s, review_guidelines: customized ? guide.trim() : "" });
  let dirty = $derived(JSON.stringify(result()) !== JSON.stringify(settings));
  function save() {
    onsave(result());
  }

  function setTheme(t: Settings["theme"]) {
    s.theme = t;
    onpreview(t);
  }
  function cancel() {
    onpreview(settings.theme);
    onclose();
  }
  function keys(e: KeyboardEvent) {
    if (e.key === "Escape") cancel();
    else if ((e.metaKey || e.ctrlKey) && e.key === "s") { e.preventDefault(); if (dirty) save(); }
  }
  const mb = (b: number) => (b / 1_048_576).toFixed(1);
</script>

{#snippet num(get: () => number, set: (v: number) => void, unit: string, min: number, max?: number)}
  <span class="num"><input type="number" {min} {max} value={get()} oninput={(e) => !isNaN(e.currentTarget.valueAsNumber) && set(e.currentTarget.valueAsNumber)} /><em>{unit}</em></span>
{/snippet}

<svelte:window onkeydown={keys} />
<div class="backdrop" role="presentation" onclick={cancel} transition:fade={{ duration: 200 }}>
  <div class="dlg" role="dialog" aria-label="Settings" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}
    transition:scale={{ start: 0.97, duration: 260, easing: cubicOut }}>
    <nav>
      <div class="ntitle">Settings</div>
      {#each pages as [id, label, icon] (id)}
        <button class:on={page === id} onclick={() => (page = id)}>
          <Icon name={icon} size={15} />{label}
          {#if id === "general" && updates.available}<span class="udot" title="Update available"></span>{/if}
          {#if id === "prompt" && customized}<span class="ntag">custom</span>{/if}
        </button>
      {/each}
      <div class="nsp"></div>
      <div class="nver mono">Peerlens {updates.version ? `v${updates.version}` : ""}</div>
    </nav>

    <div class="main">
      <div class="mh">
        <h2>{pages.find((p) => p[0] === page)?.[1]}</h2>
        <button class="ghost icon" onclick={cancel} title="Close"><Icon name="x" size={16} /></button>
      </div>

      <div class="body">
        {#if page === "general"}
          <h4>Appearance</h4>
          <div class="themes">
            {#each themes as [v, l, ic] (v)}
              <button class="theme" class:on={s.theme === v} onclick={() => setTheme(v)}>
                <span class="prev prev-{v}"><i></i><i></i><i></i></span>
                <span class="tl"><Icon name={ic} size={13} />{l}</span>
              </button>
            {/each}
          </div>

          <h4>Updates</h4>
          <div class="card">
            <div class="row">
              <div class="rl"><span>Peerlens {updates.version ? `v${updates.version}` : ""}</span>
                <small>{updates.checked && !updates.available ? "You're up to date." : "Checks GitHub releases for a newer version."}</small></div>
              <button onclick={checkForUpdates} disabled={updates.checking || updates.installing}>
                <Icon name="refresh" size={13} class={updates.checking ? "spin" : ""} />{updates.checking ? "Checking…" : "Check now"}
              </button>
            </div>
            {#if updates.error}<div class="row"><div class="uerr"><Icon name="alert" size={13} /><span>{updates.error}</span></div></div>{/if}
            {#if updates.available}
              {@const u = updates.available}
              <div class="row upd">
                <div class="uh"><b>v{u.version} is available</b><span class="dim">{u.date ?? ""}</span></div>
                {#if u.notes.trim()}<div class="notes"><Markdown text={u.notes} /></div>{/if}
                {#if updates.installing}
                  <div class="ubar"><span style:width={updates.progress < 0 ? "100%" : `${Math.round(updates.progress * 100)}%`} class:indet={updates.progress < 0}></span></div>
                  <small>{updates.progress >= 1 ? "Installing… the app will restart" : "Downloading…"}</small>
                {:else if u.can_install}
                  <button class="primary" onclick={installUpdate}><Icon name="sparkles" size={14} />Update & restart</button>
                {:else}
                  <button onclick={() => api.openUrl(u.release_url)}><Icon name="external" size={14} />Download v{u.version}</button>
                  <small>This build can't update itself (built from source). Download the new version instead.</small>
                {/if}
              </div>
            {/if}
          </div>

          <h4>Diff</h4>
          <div class="card">
            <div class="row">
              <div class="rl"><span>Collapse large files</span><small>Files with more diff lines start collapsed.</small></div>
              {@render num(() => s.collapse_lines, (v) => (s.collapse_lines = v), "lines", 50)}
            </div>
          </div>

        {:else if page === "review"}
          <h4>Defaults for new reviews</h4>
          <div class="card">
            <div class="row">
              <div class="rl"><span>Agent</span><small>Which CLI runs the review.</small></div>
              <Menu items={agentItems} value={s.default_agent} onselect={(v) => (s.default_agent = v)} align="right" width={220} />
            </div>
            <div class="row">
              <div class="rl"><span>Claude model</span></div>
              <Menu items={modelItems} value={s.default_model} onselect={(v) => (s.default_model = v)} align="right" width={260} />
            </div>
            <div class="row">
              <div class="rl"><span>Claude effort</span><small>Higher thinks longer and uses more of your limits.</small></div>
              <Menu items={EFFORTS} value={s.default_effort ?? ""} onselect={(v) => (s.default_effort = v)} align="right" width={270} />
            </div>
            {#if codex}
              <div class="row">
                <div class="rl"><span>Codex model</span></div>
                <Menu items={codexModelItems} value={s.default_codex_model ?? ""} onselect={(v) => (s.default_codex_model = v)} align="right" width={300} />
              </div>
            {/if}
            <div class="row">
              <div class="rl"><span>Review language</span><small>Language of findings and summaries.</small></div>
              <Menu items={langItems} value={s.review_language} onselect={(v) => (s.review_language = v)} align="right" width={240}>
                {#snippet footer(close)}
                  <form class="custom" onsubmit={(e) => { e.preventDefault(); if (customLang.trim()) { s.review_language = customLang.trim(); customLang = ""; close(); } }}>
                    <input bind:value={customLang} placeholder="Other language…" />
                    <button type="submit" class="primary">Use</button>
                  </form>
                {/snippet}
              </Menu>
            </div>
          </div>

          <h4>Large merge requests</h4>
          <div class="card">
            <div class="row">
              <div class="rl"><span>Review file by file above</span><small>Bigger diffs are split into several agent runs.</small></div>
              {@render num(() => s.max_review_bytes, (v) => (s.max_review_bytes = v), "bytes", 1000)}
            </div>
            <div class="row">
              <div class="rl"><span>Agent timeout</span><small>One agent run is stopped after this long.</small></div>
              {@render num(() => s.agent_timeout_secs, (v) => (s.agent_timeout_secs = v), "sec", 30)}
            </div>
          </div>

        {:else if page === "prompt"}
          <p class="hint">What the agent checks and how it writes findings. Security rules (MR text is data, not instructions), tool limits and the output format are always added by Peerlens and can't be changed here.</p>
          <textarea class="guide" spellcheck="false" bind:value={guide}></textarea>
          <div class="gbtns">
            {#if customized}<span class="pill stale">custom</span>{:else}<span class="dim small">Built-in default</span>{/if}
            <button onclick={() => (guide = defaultGuide)} disabled={!customized}><Icon name="refresh" size={13} />Reset to default</button>
          </div>

        {:else if page === "limits"}
          <p class="hint">Keeps reviews human-paced so your Claude subscription isn't hammered: one review at a time, a pause between parts, and an automatic stop near your plan limits.</p>
          <div class="card">
            <div class="row">
              <div class="rl"><span>Pause at Claude usage</span><small>Reviews stop when any plan window reaches this.</small></div>
              {@render num(() => s.max_usage_pct, (v) => (s.max_usage_pct = v), "%", 10, 100)}
            </div>
            <div class="row">
              <div class="rl"><span>Max reviews per hour</span></div>
              {@render num(() => s.max_reviews_per_hour, (v) => (s.max_reviews_per_hour = v), "", 1)}
            </div>
            <div class="row">
              <div class="rl"><span>Max agent runs per review</span><small>Caps how many parts a big MR is split into.</small></div>
              {@render num(() => s.max_chunks, (v) => (s.max_chunks = v), "", 1)}
            </div>
            <div class="row">
              <div class="rl"><span>Pause between parts</span></div>
              {@render num(() => s.chunk_delay_secs, (v) => (s.chunk_delay_secs = v), "sec", 0)}
            </div>
          </div>

        {:else if page === "repo"}
          <p class="hint">When enabled in the review panel, the repository at the MR head is downloaded through glab (GET) and unpacked into a folder the agent may only read. It stays in a local cache so discussions of the review's findings read the same code; the least recently used snapshots are removed to fit the limit below, and any unused for 14 days.</p>
          <div class="card">
            <div class="row">
              <div class="rl"><span>Max archive size</span><small>Larger repos are not downloaded.</small></div>
              {@render num(() => s.repo_max_mb, (v) => (s.repo_max_mb = v), "MB", 1)}
            </div>
            <div class="row">
              <div class="rl"><span>Keep snapshots up to</span></div>
              {@render num(() => s.repo_cache_mb, (v) => (s.repo_cache_mb = v), "MB", 100)}
            </div>
            <div class="row">
              <div class="rl"><span>Cache</span>
                <small>{cache ? `${cache.count} snapshot${cache.count === 1 ? "" : "s"} · ${mb(cache.bytes)} MB` : "…"}</small></div>
              <button onclick={clearCache} disabled={!cache?.count}><Icon name="trash" size={13} />Clear</button>
            </div>
            {#if cacheErr}<div class="row"><span class="err small">{cacheErr}</span></div>{/if}
          </div>
          <h4>Never unpack these files</h4>
          <textarea rows="6" spellcheck="false" value={s.repo_exclude.join("\n")}
            oninput={(e) => (s.repo_exclude = e.currentTarget.value.split("\n").map((x) => x.trim()).filter(Boolean))}></textarea>
          <p class="hint below">One pattern per line, <code>*</code> and <code>?</code> allowed.</p>

        {:else}
          <h4>Agents</h4>
          <div class="card">
            {#each agents as a (a.kind)}
              <div class="row">
                <span class="aico" style:color={a.kind === "claude" ? "var(--orange)" : undefined}><Icon name={a.kind} size={16} /></span>
                <div class="rl"><span>{a.name} <span class="dim mono small">{a.version}</span></span><small class="mono">{a.path}</small></div>
                {#if a.logged_in === false}<span class="state bad">Not logged in</span>{:else}<span class="state ok">Ready</span>{/if}
              </div>
            {:else}
              <div class="row"><div class="rl"><span>No agent found</span><small>Install Claude Code or Codex, or set a path below.</small></div></div>
            {/each}
          </div>

          <h4>GitLab</h4>
          <div class="card">
            <div class="row"><div class="rl"><span>Host</span></div><input bind:value={s.host} placeholder="glab default host" spellcheck="false" /></div>
            <div class="row"><div class="rl"><span>Group for “All”</span><small>Where the All list looks for open MRs.</small></div><input bind:value={s.mr_group} placeholder="all your top-level groups" spellcheck="false" /></div>
          </div>

          <h4>Binary paths</h4>
          <div class="card">
            <div class="row"><div class="rl"><span>glab</span></div><input class="mono" bind:value={s.glab_path} placeholder="auto-detect" spellcheck="false" /></div>
            <div class="row"><div class="rl"><span>claude</span></div><input class="mono" bind:value={s.claude_path} placeholder="auto-detect" spellcheck="false" /></div>
            <div class="row"><div class="rl"><span>codex</span></div><input class="mono" bind:value={s.codex_path} placeholder="auto-detect" spellcheck="false" /></div>
          </div>
        {/if}
      </div>

      <div class="btns">
        <span class="secnote" title="Agents run without tools in an empty folder, with Git/GitLab credentials removed."><Icon name="shield" size={13} />GitLab access is read-only (GET via glab)</span>
        {#if dirty}<span class="dirty" transition:fade={{ duration: 200 }}>Unsaved changes</span>{/if}
        <button onclick={cancel}>Cancel</button>
        <button class="primary" onclick={save} disabled={!dirty} title="Save ({kb('S')})">Save</button>
      </div>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(8, 8, 14, .5); backdrop-filter: blur(3px); display: grid; place-items: center; z-index: 100; }
  .dlg {
    background: var(--bg1); border-radius: 16px; box-shadow: var(--shadow); width: min(820px, 94vw); height: min(620px, 88vh);
    display: grid; grid-template-columns: 200px minmax(0, 1fr); overflow: hidden;
  }

  nav { background: var(--bg0); border-right: 1px solid var(--border); padding: 16px 10px 12px; display: flex; flex-direction: column; gap: 2px; }
  .ntitle { font-weight: 700; font-size: 14px; padding: 2px 10px 12px; }
  nav button { justify-content: flex-start; gap: 9px; border: none; background: none; padding: 7px 10px; border-radius: 8px; color: var(--text2); font-size: 13px; position: relative; }
  nav button :global(svg) { color: var(--text3); }
  nav button:hover:not(:disabled) { background: var(--bg2); color: var(--text); }
  nav button:active:not(:disabled) { transform: none; }
  nav button.on { background: var(--bg3); color: var(--text); font-weight: 600; }
  nav button.on :global(svg) { color: var(--accent); }
  .udot { width: 7px; height: 7px; border-radius: 50%; background: var(--accent); margin-left: auto; }
  .ntag { margin-left: auto; font-size: 10px; color: var(--orange); }
  .nsp { flex: 1; }
  .nver { font-size: 11px; color: var(--text3); padding: 0 10px; }

  .main { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  .mh { display: flex; align-items: center; padding: 14px 14px 4px 24px; }
  h2 { margin: 0; font-size: 17px; font-weight: 700; flex: 1; letter-spacing: -.01em; }
  .body { flex: 1; min-height: 0; overflow-y: auto; padding: 6px 24px 20px; display: flex; flex-direction: column; }
  h4 { margin: 16px 0 7px 2px; font-size: 11px; font-weight: 650; text-transform: uppercase; letter-spacing: .06em; color: var(--text3); }
  h4:first-child { margin-top: 6px; }

  /* Grouped rows, like system settings. */
  .card { background: var(--bg2); border: 1px solid var(--border); border-radius: 12px; }
  .row { display: flex; align-items: center; gap: 12px; padding: 10px 14px; min-height: 48px; }
  .row + .row { border-top: 1px solid var(--border); }
  .rl { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .rl > span { font-size: 13px; }
  .rl small { font-size: 11.5px; color: var(--text3); line-height: 1.4; overflow: hidden; text-overflow: ellipsis; }
  .row input:not([type="number"]), .row :global(.menu) { width: 230px; flex: none; }
  .row :global(.menu .trigger) { width: 100%; justify-content: flex-start; }
  .row :global(.menu .trigger .lbl) { flex: 1; text-align: left; }
  .row > button { flex: none; font-size: 12px; }

  /* Same width everywhere; the unit column is fixed so the numbers line up. */
  .num { display: inline-flex; align-items: center; flex: none; width: 150px; background: var(--bg3); border: 1px solid var(--border); border-radius: 8px; transition: border-color var(--t) var(--ease); }
  .num:focus-within { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-bg); }
  .num input { flex: 1; min-width: 0; width: auto; border: none; background: none; text-align: right; font-variant-numeric: tabular-nums; appearance: textfield; -moz-appearance: textfield; }
  .num input:focus { background: none; }
  .num input:focus-visible { outline: none; } /* the field's own ring is shown instead */
  .num input::-webkit-inner-spin-button, .num input::-webkit-outer-spin-button { -webkit-appearance: none; margin: 0; }
  .num em { flex: none; width: 44px; font-style: normal; font-size: 11.5px; color: var(--text3); padding-left: 2px; }

  .themes { display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px; }
  .theme { flex-direction: column; align-items: stretch; gap: 8px; padding: 8px; border-radius: 12px; background: var(--bg2); border: 1px solid var(--border); }
  .theme:hover:not(:disabled) { background: var(--bg3); }
  .theme.on { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-bg); }
  .tl { display: flex; align-items: center; justify-content: center; gap: 6px; font-size: 12.5px; color: var(--text2); }
  .theme.on .tl { color: var(--text); font-weight: 600; }
  /* Tiny app previews: sidebar, content, accent line. Fixed colors on purpose. */
  .prev { position: relative; height: 64px; border-radius: 8px; overflow: hidden; display: block; border: 1px solid var(--border); }
  .prev i { position: absolute; display: block; border-radius: 3px; }
  .prev i:nth-child(1) { left: 0; top: 0; bottom: 0; width: 30%; border-radius: 0; }
  .prev i:nth-child(2) { left: 38%; right: 10%; top: 14px; height: 6px; }
  .prev i:nth-child(3) { left: 38%; right: 30%; top: 28px; height: 6px; }
  .prev-dark { background: #1b1c24; } .prev-dark i:nth-child(1) { background: #121219; } .prev-dark i:nth-child(2) { background: #8d9bff; } .prev-dark i:nth-child(3) { background: #363745; }
  .prev-light { background: #ffffff; } .prev-light i:nth-child(1) { background: #f2f2f6; } .prev-light i:nth-child(2) { background: #5563f0; } .prev-light i:nth-child(3) { background: #e3e4ee; }
  .prev-system { background: linear-gradient(135deg, #ffffff 50%, #1b1c24 50%); }
  .prev-system i:nth-child(1) { background: linear-gradient(135deg, #f2f2f6 50%, #121219 50%); } .prev-system i:nth-child(2) { background: #6f7dff; } .prev-system i:nth-child(3) { background: #8a8d9c; opacity: .5; }

  .upd { flex-direction: column; align-items: flex-start; gap: 10px; }
  .uh { display: flex; gap: 10px; align-items: baseline; width: 100%; }
  .uh .dim { margin-left: auto; font-size: 12px; }
  .notes { max-height: 200px; overflow-y: auto; width: 100%; font-size: 12.5px; }
  .ubar { width: 100%; height: 5px; background: var(--bg4); border-radius: 3px; overflow: hidden; }
  .ubar span { display: block; height: 100%; background: var(--accent); transition: width 200ms var(--ease); }
  .ubar span.indet { opacity: .6; }
  .upd small { font-size: 11.5px; color: var(--text3); }
  .uerr { display: flex; gap: 7px; align-items: flex-start; color: var(--red); font-size: 12px; overflow-wrap: anywhere; }

  .aico { width: 30px; height: 30px; border-radius: 8px; background: var(--bg3); display: grid; place-items: center; flex: none; }
  .state { font-size: 11px; font-weight: 600; padding: 2px 9px; border-radius: 999px; flex: none; }
  .state.ok { color: var(--green); background: color-mix(in srgb, var(--green) 13%, transparent); }
  .state.bad { color: var(--orange); background: color-mix(in srgb, var(--orange) 14%, transparent); }

  .custom { display: flex; gap: 6px; }
  .custom input { flex: 1; min-width: 0; width: auto; }
  textarea { width: 100%; font-family: var(--mono); font-size: 12px; line-height: 1.5; resize: vertical; min-height: 90px; background: var(--bg2); }
  .guide { flex: 1; min-height: 240px; resize: none; }
  .gbtns { display: flex; align-items: center; justify-content: space-between; margin-top: 10px; }
  .gbtns button { gap: 6px; font-size: 12px; }
  .hint { margin: 4px 0 12px; font-size: 12px; color: var(--text3); line-height: 1.55; }
  .hint.below { margin: 6px 0 0; }
  .hint code { font-size: 11px; background: var(--bg3); padding: 0 4px; border-radius: 4px; }
  .small { font-size: 11.5px; }

  .btns { display: flex; align-items: center; gap: 8px; padding: 12px 20px 14px 24px; border-top: 1px solid var(--border); }
  .secnote { display: flex; gap: 6px; align-items: center; font-size: 11.5px; color: var(--text3); margin-right: auto; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .secnote :global(svg) { color: var(--green); flex: none; }
  .dirty { font-size: 11.5px; color: var(--orange); white-space: nowrap; }
</style>
