<script lang="ts">
  import { icons, type IconName } from "../lib/icons";
  let { name, size = 16, class: cls = "", style = "" }: { name: IconName; size?: number; class?: string; style?: string } = $props();
</script>

<!-- Static, trusted SVG markup from lib/icons.ts only. -->
<svg class="ic {cls}" {style} width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor"
  stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{@html icons[name]}</svg>

<style>
  .ic { flex: none; display: block; }
</style>
