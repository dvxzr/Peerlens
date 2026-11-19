<script lang="ts">
  import { parse, type Block, type Inline } from "../lib/markdown";
  import { api } from "../lib/api";
  import Icon from "./Icon.svelte";
  import type { IconName } from "../lib/icons";

  let { text }: { text: string } = $props();
  let blocks = $derived(parse(text));
  const open = (e: MouseEvent, href: string) => { e.preventDefault(); api.openUrl(href).catch(() => {}); };
  const file = (u: string) => decodeURIComponent(u.split(/[?#]/)[0].split("/").pop() || "image");
  // Status badges (Sonar, CI bots) use tiny remote images named passed.svg, failed.svg, …
  // Show a built-in icon instead of loading them; other images stay a click-to-open chip.
  const BADGES: [RegExp, IconName, string][] = [
    [/pass|success|(^|[-_])ok([-_.]|$)|check/, "check", "var(--green)"],
    [/fail|error|critical|blocker/, "x", "var(--red)"],
    [/accept/, "clock", "var(--orange)"],
    [/warn|major/, "alert", "var(--yellow)"],
    [/info|minor/, "circleDot", "var(--blue)"],
    [/skip|neutral|unknown/, "circleDot", "var(--text3)"],
  ];
  function badge(href: string) {
    const f = file(href).toLowerCase();
    if (!/\.(svg|png|gif)$/.test(f)) return null;
    return BADGES.find(([re]) => re.test(f.replace(/\.\w+$/, ""))) ?? null;
  }
</script>

{#snippet inl(items: Inline[])}{#each items as n, i (i)}{#if n.t === "text"}{n.v}{:else if n.t === "code"}<code>{n.v}</code>{:else if n.t === "strong"}<strong>{@render inl(n.c)}</strong>{:else if n.t === "em"}<em>{@render inl(n.c)}</em>{:else if n.t === "del"}<del>{@render inl(n.c)}</del>{:else if n.t === "link"}<a href={n.href} title={n.href} onclick={(e) => open(e, n.href)}>{@render inl(n.c)}</a>{:else if n.t === "image" && badge(n.href)}{@const bd = badge(n.href)!}<span class="badge" title={n.alt || file(n.href)}><Icon name={bd[1]} size={15} style="color:{bd[2]}" /></span>{:else if n.t === "image"}<a class="img" href={n.href} title="Remote image (not loaded): {n.href}" onclick={(e) => open(e, n.href)}><Icon name="file" size={11} />{n.alt || file(n.href)}</a>{:else}<br />{/if}{/each}{/snippet}

{#snippet blk(items: Block[])}
  {#each items as b, i (i)}
    {#if b.t === "p"}<p>{@render inl(b.c)}</p>
    {:else if b.t === "h"}<div class="h h{b.level}">{@render inl(b.c)}</div>
    {:else if b.t === "code"}<pre><code>{b.v}</code></pre>
    {:else if b.t === "quote"}<blockquote>{@render blk(b.c)}</blockquote>
    {:else if b.t === "hr"}<hr />
    {:else if b.t === "list"}
      <ul class:ol={b.ordered}>
        {#each b.items as it, k (k)}
          <li style:margin-left="{it.depth * 16}px" class:task={it.task}>
            {#if it.task}<span class="cb" class:done={it.done}>{#if it.done}<Icon name="check" size={10} />{/if}</span>{:else}<span class="bullet">{b.ordered ? `${k + 1}.` : "•"}</span>{/if}
            <span>{@render inl(it.c)}</span>
          </li>
        {/each}
      </ul>
    {:else if b.t === "table"}
      <div class="tw"><table>
        <thead><tr>{#each b.head as c, k (k)}<th>{@render inl(c)}</th>{/each}</tr></thead>
        <tbody>{#each b.rows as r, k (k)}<tr>{#each r as c, j (j)}<td>{@render inl(c)}</td>{/each}</tr>{/each}</tbody>
      </table></div>
    {/if}
  {/each}
{/snippet}

<div class="md">{@render blk(blocks)}</div>

<style>
  .md { line-height: 1.55; overflow-wrap: anywhere; }
  .md > :global(:first-child) { margin-top: 0; }
  .md > :global(:last-child) { margin-bottom: 0; }
  p { margin: 0 0 8px; }
  .h { font-weight: 650; margin: 12px 0 6px; line-height: 1.3; }
  .h1 { font-size: 1.35em; } .h2 { font-size: 1.2em; } .h3 { font-size: 1.08em; } .h4, .h5, .h6 { font-size: 1em; }
  code { font-family: var(--mono); font-size: .9em; background: var(--bg4); padding: 1px 5px; border-radius: 5px; }
  pre { background: var(--bg1); border: 1px solid var(--border); border-radius: 8px; padding: 8px 10px; overflow-x: auto; margin: 6px 0 10px; }
  pre code { background: none; padding: 0; font-size: 12px; white-space: pre; }
  blockquote { margin: 6px 0 10px; padding: 2px 0 2px 12px; border-left: 3px solid var(--border2); color: var(--text2); }
  hr { border: none; border-top: 1px solid var(--border); margin: 12px 0; }
  ul { list-style: none; padding: 0; margin: 0 0 8px; }
  li { display: flex; gap: 8px; align-items: baseline; margin: 2px 0; }
  .bullet { color: var(--text3); min-width: 10px; flex: none; }
  .cb { width: 13px; height: 13px; border: 1.5px solid var(--border2); border-radius: 4px; display: inline-grid; place-items: center; flex: none; transform: translateY(2px); }
  .cb.done { background: var(--accent); border-color: var(--accent); color: var(--on-accent); }
  a { color: var(--accent); text-decoration: none; }
  a:hover { text-decoration: underline; }
  a.img { display: inline-flex; align-items: center; gap: 4px; font-size: 11px; color: var(--text2); background: var(--bg3); border: 1px solid var(--border); border-radius: 5px; padding: 0 6px; vertical-align: 1px; }
  .badge { display: inline-flex; vertical-align: -3px; margin-right: 4px; }
  .badge :global(svg) { stroke-width: 2.4; }
  .tw { overflow-x: auto; margin: 6px 0 10px; }
  table { border-collapse: collapse; font-size: 12.5px; }
  th, td { border: 1px solid var(--border); padding: 4px 9px; text-align: left; vertical-align: top; }
  th { background: var(--bg3); font-weight: 600; }
</style>
