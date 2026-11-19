<script lang="ts">
  import type { Commit, FileDiff } from "../lib/types";
  import { ago, fmtDate } from "../lib/time";
  import { api, errText } from "../lib/api";
  import DiffView from "./DiffView.svelte";
  import Icon from "./Icon.svelte";

  let { commits, projectId, collapseLines }: { commits: Commit[]; projectId: number; collapseLines: number } = $props();
  let sel = $state<Commit | null>(null);
  let diff = $state<FileDiff[] | null>(null);
  let err = $state("");
  const cache = new Map<string, FileDiff[]>();

  async function pick(c: Commit) {
    sel = c; err = ""; diff = cache.get(c.id) ?? null;
    if (diff) return;
    try {
      const d = await api.commitDiff(projectId, c.id);
      cache.set(c.id, d);
      if (sel?.id === c.id) diff = d;
    } catch (e) { err = errText(e); }
  }
</script>

<div class="wrap">
  <div class="list" class:compact={!!sel}>
    {#each commits as c (c.id)}
      <button class="row" class:sel={sel?.id === c.id} onclick={() => pick(c)}>
        <Icon name="commit" size={15} class="ci" />
        <span class="title">{c.title}</span>
        <span class="who dim">{c.author_name}</span>
        <span class="when dim" title={fmtDate(c.authored_date)}>{ago(c.authored_date)}</span>
        <span class="sha mono">{c.short_id}</span>
      </button>
    {/each}
  </div>
  {#if sel}
    <div class="diff">
      {#if err}<div class="err pad">{err}</div>
      {:else if !diff}<div class="pad"><div class="skeleton" style="height:14px;width:60%"></div></div>
      {:else}<DiffView files={diff} {collapseLines} />{/if}
    </div>
  {:else}
    <div class="hint dim">Pick a commit to see its changes.</div>
  {/if}
</div>

<style>
  .wrap { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .list { overflow-y: auto; flex: none; padding: 8px 10px; border-bottom: 1px solid var(--border); max-height: 100%; }
  .list.compact { max-height: 36%; }
  .row { display: flex; gap: 10px; width: 100%; justify-content: flex-start; text-align: left; border: 1px solid transparent; background: none; padding: 7px 10px; border-radius: 9px; }
  .row:hover:not(:disabled) { background: var(--bg2); }
  .row.sel { background: var(--bg2); border-color: var(--border2); }
  .row :global(.ci) { color: var(--text3); }
  .title { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500; }
  .who, .when { font-size: 12px; white-space: nowrap; }
  .sha { color: var(--accent); background: var(--accent-bg); border-radius: 6px; padding: 0 6px; font-size: 11px; }
  .diff { flex: 1; min-height: 0; }
  .hint { padding: 24px; text-align: center; }
  .pad { padding: 16px; }
</style>
