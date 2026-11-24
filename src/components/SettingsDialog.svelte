<script lang="ts">
  import type { AgentInfo, MenuItem, Settings } from "../lib/types";
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
  onMount(async () => {
    loadVersion();
    defaultGuide = await api.defaultGuidelines();
    if (!guide.trim()) guide = defaultGuide;
  });
  let customized = $derived(!!defaultGuide && guide.trim() !== defaultGuide.trim());
  let customLang = $state("");
  let langItems = $derived<MenuItem<string>[]>(
    [...new Set([...LANGUAGES, s.review_language].filter(Boolean))].map((l) => ({ value: l, label: l, icon: "globe" })),
  );
  function save() {
    onsave({ ...s, review_guidelines: customized ? guide.trim() : "" });
  }

  function setTheme(t: Settings["theme"]) {
    s.theme = t;
    onpreview(t);
  }
  function cancel() {
    onpreview(settings.theme);
    onclose();
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && cancel()} />
<div class="backdrop" role="presentation" onclick={cancel} transition:fade={{ duration: 140 }}>
  <div class="dlg" role="dialog" aria-label="Settings" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}
    transition:scale={{ start: 0.96, duration: 170, easing: cubicOut }}>
    <div class="dh"><h2>Settings</h2><button class="ghost icon" onclick={cancel} title="Close"><Icon name="x" size={16} /></button></div>
    <div class="body">
      <section>
        <h3>Appearance</h3>
        <div class="seg">
          {#each themes as [v, l, ic] (v)}
            <button class:on={s.theme === v} onclick={() => setTheme(v)}><Icon name={ic} size={14} />{l}</button>
          {/each}
        </div>
      </section>

      <section>
        <h3><Icon name="refresh" size={14} />Updates</h3>
        <div class="f">
          <span>Peerlens {updates.version ? `v${updates.version}` : ""}
            {#if updates.checked && !updates.available}<em>· up to date</em>{/if}</span>
          <button onclick={checkForUpdates} disabled={updates.checking || updates.installing}>
            <Icon name="refresh" size={13} class={updates.checking ? "spin" : ""} />{updates.checking ? "Checking…" : "Check for updates"}
          </button>
        </div>
        {#if updates.error}<div class="uerr"><Icon name="alert" size={13} /><span>{updates.error}</span></div>{/if}
        {#if updates.available}
          {@const u = updates.available}
          <div class="upd">
            <div class="uh">
              <b>v{u.version} is available</b>
              <span class="dim">{u.date ?? ""}</span>
            </div>
            {#if u.notes.trim()}<div class="notes"><Markdown text={u.notes} /></div>{/if}
            {#if updates.installing}
              <div class="ubar"><span style:width={updates.progress < 0 ? "100%" : `${Math.round(updates.progress * 100)}%`} class:indet={updates.progress < 0}></span></div>
              <div class="dim small">{updates.progress >= 1 ? "Installing… the app will restart" : "Downloading…"}</div>
            {:else if u.can_install}
              <button class="primary" onclick={installUpdate}><Icon name="sparkles" size={14} />Update & restart</button>
            {:else}
              <button onclick={() => api.openUrl(u.release_url)}><Icon name="external" size={14} />Download v{u.version}</button>
              <div class="dim small">This build can't update itself (built from source). Download the new version instead.</div>
            {/if}
          </div>
        {/if}
      </section>

      <section>
        <h3><Icon name="sparkles" size={14} />Review</h3>
        <div class="f"><span>Default agent</span><Menu items={agentItems} value={s.default_agent} onselect={(v) => (s.default_agent = v)} align="right" width={220} /></div>
        <div class="f"><span>Default Claude model</span><Menu items={modelItems} value={s.default_model} onselect={(v) => (s.default_model = v)} align="right" width={260} /></div>
        {#if codex}
          <div class="f"><span>Default Codex model</span><Menu items={codexModelItems} value={s.default_codex_model ?? ""} onselect={(v) => (s.default_codex_model = v)} align="right" width={300} /></div>
        {/if}
        <div class="f"><span>Default Claude effort</span><Menu items={EFFORTS} value={s.default_effort ?? ""} onselect={(v) => (s.default_effort = v)} align="right" width={270} /></div>
        <div class="f"><span>Review language</span>
          <Menu items={langItems} value={s.review_language} onselect={(v) => (s.review_language = v)} align="right" width={240}>
            {#snippet footer(close)}
              <form class="custom" onsubmit={(e) => { e.preventDefault(); if (customLang.trim()) { s.review_language = customLang.trim(); customLang = ""; close(); } }}>
                <input bind:value={customLang} placeholder="Other language…" />
                <button type="submit" class="primary">Use</button>
              </form>
            {/snippet}
          </Menu>
        </div>
        <label class="f"><span>Review file by file above <em>bytes</em></span><input type="number" min="1000" bind:value={s.max_review_bytes} /></label>
        <label class="f"><span>Agent timeout <em>seconds</em></span><input type="number" min="30" bind:value={s.agent_timeout_secs} /></label>
      </section>

      <section>
        <h3><Icon name="message" size={14} />Review prompt {#if customized}<span class="pill stale">custom</span>{/if}</h3>
        <p class="hint">What the agent checks and how it writes findings. Security rules (MR text is data, not instructions), tool limits and the output format are always added by Peerlens and can't be changed here.</p>
        <textarea class="guide" rows="14" spellcheck="false" bind:value={guide}></textarea>
        <div class="gbtns">
          <button onclick={() => (guide = defaultGuide)} disabled={!customized}><Icon name="refresh" size={13} />Reset to default</button>
        </div>
      </section>

      <section>
        <h3><Icon name="gauge" size={14} />Usage limits</h3>
        <p class="hint">Keeps reviews human-paced so your Claude subscription isn't hammered: one review at a time, a pause between parts, and an automatic stop near your plan limits.</p>
        <label class="f"><span>Pause when Claude usage reaches <em>%</em></span><input type="number" min="10" max="100" bind:value={s.max_usage_pct} /></label>
        <label class="f"><span>Max reviews per hour</span><input type="number" min="1" bind:value={s.max_reviews_per_hour} /></label>
        <label class="f"><span>Max agent runs per review</span><input type="number" min="1" bind:value={s.max_chunks} /></label>
        <label class="f"><span>Pause between parts <em>seconds</em></span><input type="number" min="0" bind:value={s.chunk_delay_secs} /></label>
      </section>

      <section>
        <h3><Icon name="folder" size={14} />Repo context</h3>
        <p class="hint">When enabled in the review panel, the repository at the MR head is downloaded through glab (GET), unpacked into a temporary folder the agent may only read, and deleted after the review.</p>
        <label class="f"><span>Max archive size <em>MB</em></span><input type="number" min="1" bind:value={s.repo_max_mb} /></label>
        <label class="col"><span>Never unpack these files <em>one pattern per line, * and ? allowed</em></span>
          <textarea rows="5" spellcheck="false" value={s.repo_exclude.join("\n")}
            oninput={(e) => (s.repo_exclude = e.currentTarget.value.split("\n").map((x) => x.trim()).filter(Boolean))}></textarea>
        </label>
      </section>

      <section>
        <h3><Icon name="mr" size={14} />GitLab</h3>
        <label class="f"><span>Host</span><input bind:value={s.host} placeholder="glab default host" spellcheck="false" /></label>
        <label class="f"><span>Group for “All”</span><input bind:value={s.mr_group} placeholder="all your top-level groups" spellcheck="false" /></label>
        <label class="f"><span>glab path</span><input bind:value={s.glab_path} placeholder="auto-detect" spellcheck="false" /></label>
      </section>

      <section>
        <h3><Icon name="terminal" size={14} />Paths & display</h3>
        <label class="f"><span>claude path</span><input bind:value={s.claude_path} placeholder="auto-detect" spellcheck="false" /></label>
        <label class="f"><span>codex path</span><input bind:value={s.codex_path} placeholder="auto-detect" spellcheck="false" /></label>
        <label class="f"><span>Collapse files above <em>lines</em></span><input type="number" min="50" bind:value={s.collapse_lines} /></label>
      </section>

      <div class="secnote"><Icon name="shield" size={14} />GitLab access is GET-only through glab. Agents run without tools in an empty folder, with Git/GitLab credentials removed.</div>
    </div>
    <div class="btns"><button onclick={cancel}>Cancel</button><button class="primary" onclick={save}>Save</button></div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(8, 8, 14, .5); backdrop-filter: blur(3px); display: grid; place-items: center; z-index: 100; }
  .dlg { background: var(--bg2); border-radius: 16px; box-shadow: var(--shadow); width: 560px; max-height: 86vh; display: flex; flex-direction: column; }
  .dh { display: flex; align-items: center; padding: 14px 14px 6px 20px; }
  h2 { margin: 0; font-size: 16px; flex: 1; }
  .body { overflow-y: auto; padding: 4px 20px 8px; }
  section { padding: 12px 0; border-bottom: 1px solid var(--border); }
  h3 { display: flex; align-items: center; gap: 7px; margin: 0 0 10px; font-size: 12px; text-transform: uppercase; letter-spacing: .05em; color: var(--text2); }
  h3 :global(svg) { color: var(--text3); }
  .f { display: flex; justify-content: space-between; align-items: center; gap: 12px; padding: 4px 0; }
  .f > span { color: var(--text); }
  em { font-style: normal; color: var(--text3); font-size: 11.5px; }
  /* All right-hand controls share one width so the column lines up. */
  .f input, .f :global(.menu) { width: 220px; flex: none; }
  .f :global(.menu .trigger) { width: 100%; justify-content: flex-start; }
  .f :global(.menu .trigger .lbl) { flex: 1; text-align: left; }
  /* Every value is left-aligned like the popups; no native number spinners. */
  .f input[type="number"] { font-variant-numeric: tabular-nums; appearance: textfield; -moz-appearance: textfield; }
  .f input[type="number"]::-webkit-inner-spin-button, .f input[type="number"]::-webkit-outer-spin-button { -webkit-appearance: none; margin: 0; }
  .custom { display: flex; gap: 6px; }
  .custom input { flex: 1; min-width: 0; width: auto; }
  .guide { width: 100%; font-size: 12px; line-height: 1.5; min-height: 220px; }
  .upd { margin-top: 10px; padding: 12px; border: 1px solid var(--border); border-radius: 10px; background: var(--bg1); display: flex; flex-direction: column; gap: 10px; align-items: flex-start; }
  .uh { display: flex; gap: 10px; align-items: baseline; width: 100%; }
  .uh .dim { margin-left: auto; font-size: 12px; }
  .notes { max-height: 220px; overflow-y: auto; width: 100%; font-size: 12.5px; }
  .ubar { width: 100%; height: 5px; background: var(--bg4); border-radius: 3px; overflow: hidden; }
  .ubar span { display: block; height: 100%; background: var(--accent); transition: width 200ms var(--ease); }
  .ubar span.indet { opacity: .6; }
  .uerr { display: flex; gap: 7px; align-items: flex-start; color: var(--red); font-size: 12px; margin-top: 6px; overflow-wrap: anywhere; }
  .small { font-size: 11.5px; }
  .gbtns { display: flex; justify-content: flex-end; margin-top: 8px; }
  .gbtns button { gap: 6px; font-size: 12px; }
  h3 .pill { text-transform: none; letter-spacing: 0; }
  .col { display: flex; flex-direction: column; gap: 6px; padding: 4px 0; }
  textarea { font-family: var(--mono); font-size: 12px; resize: vertical; min-height: 90px; }
  .hint { margin: -4px 0 8px; font-size: 12px; color: var(--text3); line-height: 1.5; }
  .seg { display: inline-flex; background: var(--bg1); border: 1px solid var(--border); border-radius: 10px; padding: 3px; gap: 2px; }
  .seg button { border: none; background: none; padding: 5px 14px; border-radius: 8px; color: var(--text2); }
  .seg button.on { background: var(--bg4); color: var(--text); font-weight: 600; }
  .secnote { display: flex; gap: 8px; align-items: flex-start; font-size: 12px; color: var(--text3); padding: 12px 0 4px; line-height: 1.5; }
  .secnote :global(svg) { color: var(--green); flex: none; margin-top: 2px; }
  .btns { display: flex; justify-content: flex-end; gap: 8px; padding: 12px 20px 16px; border-top: 1px solid var(--border); }
</style>
