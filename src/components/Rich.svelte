<script lang="ts">
  // Plain text with `inline code` spans. Built from text nodes only — never {@html}.
  let { text }: { text: string } = $props();
  let parts = $derived(text.split(/(`[^`\n]+`)/g).filter(Boolean));
</script>

{#each parts as p, i (i)}{#if p.length > 2 && p.startsWith("`") && p.endsWith("`")}<code>{p.slice(1, -1)}</code>{:else}{p}{/if}{/each}

<style>
  code { font-family: var(--mono); font-size: .92em; background: var(--bg4); padding: 1px 5px; border-radius: 5px; overflow-wrap: anywhere; }
</style>
