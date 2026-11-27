<script lang="ts" generics="T extends string">
  import type { Snippet } from "svelte";
  import { scale } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import Icon from "./Icon.svelte";
  import type { IconName } from "../lib/icons";
  import type { MenuItem } from "../lib/types";
  type Item<V> = MenuItem<V>;

  let { items, value, onselect, icon, iconColor, label, title, align = "left", footer, width }: {
    items: Item<T>[]; value: T; onselect: (v: T) => void;
    icon?: IconName; iconColor?: string; label?: string; title?: string; align?: "left" | "right";
    footer?: Snippet<[() => void]>; width?: number;
  } = $props();

  let open = $state(false);
  let active = $state(0);
  let root: HTMLDivElement | undefined = $state();
  let list: HTMLDivElement | undefined = $state();
  let current = $derived(items.find((i) => i.value === value));

  function toggle() {
    open = !open;
    if (open) {
      active = Math.max(0, items.findIndex((i) => i.value === value));
      queueMicrotask(() => list?.focus());
    }
  }
  const close = () => (open = false);
  function pick(i: Item<T>) {
    if (i.disabled) return;
    onselect(i.value);
    close();
  }
  function key(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); close(); }
    else if (e.key === "ArrowDown") { e.preventDefault(); active = (active + 1) % items.length; }
    else if (e.key === "ArrowUp") { e.preventDefault(); active = (active - 1 + items.length) % items.length; }
    else if (e.key === "Enter") { e.preventDefault(); pick(items[active]); }
  }
  // Close on outside click; listener exists only while open.
  $effect(() => {
    if (!open) return;
    const h = (e: PointerEvent) => !root?.contains(e.target as Node) && close();
    window.addEventListener("pointerdown", h, true);
    return () => window.removeEventListener("pointerdown", h, true);
  });
</script>

<div class="menu" bind:this={root}>
  <button class="trigger" class:open onclick={toggle} {title} aria-haspopup="listbox" aria-expanded={open}>
    {#if current?.icon ?? icon}<Icon name={(current?.icon ?? icon)!} size={15} style="color:{current?.color ?? iconColor ?? 'currentColor'}" />{/if}
    <span class="lbl">{label ?? current?.label ?? value}</span>
    <Icon name="chevronDown" size={13} class="chev" />
  </button>
  {#if open}
    <div class="pop" class:right={align === "right"} style:min-width={width ? `${width}px` : undefined}
      transition:scale={{ start: 0.96, duration: 200, easing: cubicOut }}>
      <div class="list" role="listbox" tabindex="-1" bind:this={list} onkeydown={key}>
        {#each items as it, i (it.value)}
          <button class="opt" class:active={i === active} class:sel={it.value === value} disabled={it.disabled}
            role="option" aria-selected={it.value === value} onpointerenter={() => (active = i)} onclick={() => pick(it)}>
            {#if it.icon}<Icon name={it.icon} size={16} style="color:{it.color ?? 'var(--text2)'}" />{/if}
            <span class="txt">
              <span class="l">{it.label}</span>
              {#if it.desc}<span class="d">{it.desc}</span>{/if}
            </span>
            <span class="mark">{#if it.value === value}<Icon name="check" size={15} />{/if}</span>
          </button>
        {/each}
      </div>
      {#if footer}<div class="foot">{@render footer(close)}</div>{/if}
    </div>
  {/if}
</div>

<style>
  .menu { position: relative; display: inline-flex; min-width: 0; }
  .trigger { gap: 7px; padding: 5px 9px 5px 10px; max-width: 100%; min-width: 0; background: var(--bg3); }
  .trigger.open { border-color: var(--accent); background: var(--bg2); }
  .lbl { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500; }
  .trigger :global(.chev) { color: var(--text3); transition: transform var(--t) var(--ease); }
  .trigger.open :global(.chev) { transform: rotate(180deg); }
  .pop {
    position: absolute; top: calc(100% + 6px); left: 0; z-index: 50; min-width: 100%; max-width: 340px;
    background: var(--bg2); border-radius: 12px; box-shadow: var(--shadow); padding: 5px; transform-origin: top left;
  }
  .pop.right { left: auto; right: 0; transform-origin: top right; }
  .list { outline: none; display: flex; flex-direction: column; gap: 1px; max-height: min(440px, 62vh); overflow-y: auto; }
  .opt {
    justify-content: flex-start; text-align: left; gap: 10px; border: none; background: transparent; border-radius: 8px;
    padding: 7px 8px; width: 100%; white-space: nowrap;
  }
  .opt:hover:not(:disabled), .opt.active { background: var(--bg4); }
  .opt:active:not(:disabled) { transform: none; }
  .txt { display: flex; flex-direction: column; flex: 1; min-width: 0; }
  .l { font-weight: 500; }
  .d { font-size: 11.5px; color: var(--text3); white-space: normal; }
  .mark { width: 16px; color: var(--accent); display: flex; }
  .foot { border-top: 1px solid var(--border); margin-top: 5px; padding: 6px 4px 2px; }
</style>
