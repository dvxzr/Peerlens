<script lang="ts">
  import type { Discussion } from "../lib/types";
  import { ago, fmtDate } from "../lib/time";
  import Icon from "./Icon.svelte";
  import Markdown from "./Markdown.svelte";

  let { discussions, onjump }: { discussions: Discussion[]; onjump: (path: string, line: number | null) => void } = $props();
  let filter = $state<"unresolved" | "resolved" | "all">("unresolved");
  let showSystem = $state(false);

  const resolvable = (d: Discussion) => d.notes.some((n) => n.resolvable);
  const resolved = (d: Discussion) => resolvable(d) && d.notes.filter((n) => n.resolvable).every((n) => n.resolved);
  let shown = $derived(
    discussions.filter((d) => {
      if (!showSystem && d.notes.every((n) => n.system)) return false;
      if (filter === "unresolved") return resolvable(d) && !resolved(d);
      if (filter === "resolved") return resolved(d);
      return true;
    }),
  );
  const initials = (n: string) => n.split(/[\s._-]+/).filter(Boolean).slice(0, 2).map((s) => s[0]!.toUpperCase()).join("");
</script>

<div class="bar">
  <div class="seg">
    {#each [["unresolved", "Unresolved"], ["resolved", "Resolved"], ["all", "All"]] as [v, l] (v)}
      <button class:on={filter === v} onclick={() => (filter = v as typeof filter)}>{l}</button>
    {/each}
  </div>
  <button class="ghost" class:active={showSystem} onclick={() => (showSystem = !showSystem)} title="Show system notes">
    <Icon name={showSystem ? "eye" : "filter"} size={14} />System notes
  </button>
  <span class="dim count">{shown.length} of {discussions.length}</span>
</div>
<div class="list">
  {#each shown as d (d.id)}
    {@const pos = d.notes[0].position}
    {@const sys = d.notes.every((n) => n.system)}
    <div class="thread" class:resolved={resolved(d)} class:system={sys}>
      {#if pos?.new_path || pos?.old_path || resolvable(d)}
        <div class="thead">
          {#if pos?.new_path || pos?.old_path}
            <button class="loc mono" onclick={() => onjump((pos.new_path ?? pos.old_path)!, pos.new_line)}>
              <Icon name="file" size={13} />{pos.new_path ?? pos.old_path}:{pos.new_line ?? pos.old_line ?? ""}
            </button>
          {/if}
          {#if resolvable(d)}
            <span class="pill {resolved(d) ? 'success' : 'stale'}"><Icon name={resolved(d) ? "checkCircle" : "circleDot"} size={12} />{resolved(d) ? "Resolved" : "Unresolved"}</span>
          {/if}
        </div>
      {/if}
      {#each d.notes as n (n.id)}
        <div class="note" class:system={n.system}>
          <span class="av">{initials(n.author.name || n.author.username)}</span>
          <div class="nb">
            <div class="who">
              <b>{n.author.name || n.author.username}</b>
              {#if n.author.username}<span class="dim uname" title="@{n.author.username}">@{n.author.username}</span>{/if}
              {#if /^(project|group)_\d+_bot/.test(n.author.username)}<span class="pill">bot</span>{/if}
              <span class="dim when" title={fmtDate(n.created_at)}>{ago(n.created_at)}</span>
            </div>
            <!-- Untrusted: rendered through the safe Markdown component (text nodes only). -->
            <div class="body">{#if n.system}{n.body}{:else}<Markdown text={n.body} />{/if}</div>
          </div>
        </div>
      {/each}
    </div>
  {:else}
    <div class="empty dim"><Icon name="message" size={22} />No discussions for this filter.</div>
  {/each}
</div>

<style>
  .bar { display: flex; gap: 10px; padding: 10px 18px; border-bottom: 1px solid var(--border); align-items: center; }
  .seg { display: inline-flex; background: var(--bg2); border: 1px solid var(--border); border-radius: 9px; padding: 2px; }
  .seg button { border: none; background: none; padding: 3px 11px; border-radius: 7px; color: var(--text2); font-size: 12px; }
  .seg button.on { background: var(--bg4); color: var(--text); font-weight: 600; }
  .ghost.active { color: var(--text); background: var(--bg3); }
  .count { margin-left: auto; font-size: 12px; }
  .list { overflow-y: auto; flex: 1; padding: 12px 18px 24px; }
  .thread { border: 1px solid var(--border); background: var(--bg2); border-radius: 12px; padding: 4px 12px; margin-bottom: 10px; }
  .thread.resolved { opacity: .72; }
  .thread.system { background: transparent; border-style: dashed; }
  .thead { display: flex; gap: 8px; align-items: center; padding: 8px 0 4px; flex-wrap: wrap; }
  .loc { gap: 5px; padding: 2px 8px; background: var(--bg3); border: none; font-size: 11.5px; color: var(--accent); }
  .note { display: flex; gap: 10px; padding: 9px 0; border-top: 1px solid var(--border); }
  .note:first-of-type, .thead + .note { border-top: none; }
  .note.system { color: var(--text2); font-size: 12px; }
  .av { width: 26px; height: 26px; border-radius: 50%; flex: none; display: grid; place-items: center; font-size: 10.5px; font-weight: 700; background: var(--bg4); color: var(--text2); }
  .nb { min-width: 0; flex: 1; }
  .who { font-size: 12.5px; display: flex; align-items: baseline; gap: 6px; min-width: 0; }
  .who b { white-space: nowrap; }
  .uname { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; max-width: 260px; }
  .when { white-space: nowrap; margin-left: auto; }
  .who .pill { line-height: 15px; font-size: 10px; align-self: center; }
  .body { overflow-wrap: anywhere; margin-top: 3px; min-width: 0; }
  .note.system .body { white-space: pre-wrap; }
  .empty { display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 40px; }
</style>
