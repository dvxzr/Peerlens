<script lang="ts">
  import type { FileDiff, Line, Marker } from "../lib/types";
  import { cached, highlightFile } from "../lib/highlight";
  import Icon from "./Icon.svelte";

  let { files, collapseLines, split = false, markers = new Map(), onmarker, onfile }: {
    files: FileDiff[]; collapseLines: number; split?: boolean;
    markers?: Map<string, Marker[]>; /* keyed by `${path}:${newLine}` */ onmarker?: (id: number) => void;
    /** Called with the path of the file at the top of the viewport while scrolling. */
    onfile?: (path: string) => void;
  } = $props();

  type Row =
    | { t: "file"; fi: number }
    | { t: "line"; fi: number; li: number; l: Line }
    /** Split view: old line index `a` on the left, new line index `b` on the right (null = blank side). */
    | { t: "pair"; fi: number; a: number | null; b: number | null }
    | { t: "note"; fi: number; text: string }
    | { t: "mark"; fi: number; m: Marker }
    | { t: "gap"; fi: number };
  const H = { file: 36, line: 20, pair: 20, note: 30, mark: 24, gap: 12 };
  const GUTTER = 122; // two 52px number columns + 18px sign
  const HALF_GUTTER = 70; // split view: one 52px number column + 18px sign

  const path = (f: FileDiff) => f.new_path || f.old_path;
  const autoCollapsed = (f: FileDiff) => f.lines.length > collapseLines || f.generated;

  let toggled = $state(new Set<number>());
  const isOpen = (fi: number) => autoCollapsed(files[fi]) === toggled.has(fi);
  function toggle(fi: number) {
    const s = new Set(toggled);
    s.has(fi) ? s.delete(fi) : s.add(fi);
    toggled = s;
  }
  $effect(() => {
    files;
    toggled = new Set();
    sx = 0;
    if (scroller) scroller.scrollTop = scroller.scrollLeft = 0;
  });

  let layout = $derived.by(() => {
    const rows: Row[] = [];
    let maxLen = 0;
    files.forEach((f, fi) => {
      rows.push({ t: "file", fi });
      if (isOpen(fi)) {
        const p = path(f);
        if (f.too_large || f.collapsed) rows.push({ t: "note", fi, text: "Diff is too large — GitLab did not send it. Open the MR in the browser." });
        else if (f.binary) rows.push({ t: "note", fi, text: "No text diff (binary or empty file)." });
        const marks = (l: Line) => {
          if (l.n != null && l.k !== "-") for (const m of markers.get(`${p}:${l.n}`) ?? []) rows.push({ t: "mark", fi, m });
        };
        // Split view: a run of removed lines sits beside the run of added lines that follows it.
        let dels: number[] = [], adds: number[] = [];
        const flush = () => {
          for (let k = 0; k < Math.max(dels.length, adds.length); k++) {
            const b = adds[k] ?? null;
            rows.push({ t: "pair", fi, a: dels[k] ?? null, b });
            if (b != null) marks(f.lines[b]);
          }
          dels = [];
          adds = [];
        };
        f.lines.forEach((l, li) => {
          // Tabs render 4 wide; this sizes the scrollable width so all rows line up.
          const len = l.t.length + 3 * (l.t.split("\t").length - 1);
          if (len > maxLen) maxLen = len;
          if (!split) {
            rows.push({ t: "line", fi, li, l });
            marks(l);
          } else if (l.k === "-") {
            if (adds.length) flush(); // "+ then -" starts a new change block
            dels.push(li);
          } else if (l.k === "+") adds.push(li);
          else {
            flush();
            if (l.k === " ") {
              rows.push({ t: "pair", fi, a: li, b: li });
              marks(l);
            } else rows.push({ t: "line", fi, li, l });
          }
        });
        flush();
      }
      rows.push({ t: "gap", fi });
    });
    const offs = new Float64Array(rows.length + 1);
    rows.forEach((r, i) => (offs[i + 1] = offs[i] + H[r.t]));
    return { rows, offs, maxLen };
  });

  let scroller: HTMLDivElement | undefined = $state();
  let scrollTop = $state(0);
  let viewH = $state(800);
  let viewW = $state(800);
  let raf = 0;
  function onScroll() {
    // At most one layout update per frame.
    if (!raf) raf = requestAnimationFrame(() => ((raf = 0), (scrollTop = scroller?.scrollTop ?? 0)));
  }

  // Sideways scrolling handled explicitly so it works the same in every WebView:
  // horizontal trackpad swipes and Shift+wheel move the code, vertical scrolling stays native.
  // Split view has no native sideways scroll: both halves shift their code together by `sx` px.
  let sx = $state(0);
  let ch100 = $state(720); // measured width of 100ch in the code font
  let maxSx = $derived(Math.max(0, (layout.maxLen * ch100) / 100 + 34 - (viewW / 2 - HALF_GUTTER)));
  $effect(() => {
    if (sx > maxSx) sx = maxSx;
  });
  function onWheel(e: WheelEvent) {
    target = -1;
    if (!scroller) return;
    const dx = e.shiftKey && !e.deltaX ? e.deltaY : e.deltaX;
    if (Math.abs(dx) <= Math.abs(e.shiftKey ? 0 : e.deltaY)) return;
    if (split) {
      if (maxSx <= 0) return;
      e.preventDefault();
      sx = Math.max(0, Math.min(maxSx, sx + dx * (e.deltaMode === 1 ? 20 : 1)));
      return;
    }
    const max = scroller.scrollWidth - scroller.clientWidth;
    if (max <= 0) return;
    e.preventDefault();
    scroller.scrollLeft = Math.max(0, Math.min(max, scroller.scrollLeft + dx * (e.deltaMode === 1 ? 20 : 1)));
  }
  $effect(() => {
    const el = scroller;
    if (!el) return;
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  });

  function firstAt(y: number) {
    const o = layout.offs;
    let lo = 0, hi = layout.rows.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (o[mid + 1] <= y) lo = mid + 1; else hi = mid;
    }
    return lo;
  }
  let win = $derived.by(() => {
    const start = Math.max(0, firstAt(scrollTop) - 30);
    const end = Math.min(layout.rows.length, firstAt(scrollTop + viewH) + 30);
    return { start, end, top: layout.offs[start], bottom: layout.offs[layout.rows.length] - layout.offs[end] };
  });

  // File under the top edge; its header is pinned once the real header has scrolled away.
  let top = $derived.by(() => {
    const i = firstAt(scrollTop);
    const r = layout.rows[i];
    if (!r) return null;
    // The next file's header pushes the pinned one up instead of overlapping it.
    let next = i + 1;
    while (next < layout.rows.length && layout.rows[next].t !== "file") next++;
    const shift = next < layout.rows.length ? Math.min(0, layout.offs[next] - scrollTop - H.file) : 0;
    return { fi: r.fi, shift, pinned: !(r.t === "file" && layout.offs[i] >= scrollTop - 1) && r.t !== "gap" };
  });
  let lastFile = -1;
  let target = -1; // file a jump asked for; it wins over "at the bottom" until the user scrolls
  $effect(() => {
    // Files near the end can't reach the top edge: at the bottom the last one (or the jump target) is current.
    const atBottom = scrollTop > 0 && scrollTop + viewH >= layout.offs[layout.rows.length] - 2;
    const fi = atBottom ? (target >= 0 ? target : files.length - 1) : top?.fi ?? -1;
    if (fi >= 0 && fi !== lastFile && files[fi]) {
      lastFile = fi;
      onfile?.(path(files[fi]));
    }
  });

  // Highlight files as they scroll into view; bump `hl` to re-render with colors.
  let hl = $state(0);
  $effect(() => {
    const seen = new Set<number>();
    for (let i = win.start; i < win.end; i++) seen.add(layout.rows[i].fi);
    for (const fi of seen) {
      const f = files[fi];
      if (f && isOpen(fi) && cached(f) === undefined) highlightFile(f).then((changed) => changed && hl++);
    }
  });
  function html(fi: number, li: number) {
    void hl; // re-run when a file finishes highlighting
    return cached(files[fi])?.[li] ?? null;
  }

  /** Scroll to a file (and optionally a new-side line), expanding it if needed. */
  export function scrollTo(p: string, line?: number | null) {
    const fi = files.findIndex((f) => f.new_path === p || f.old_path === p);
    if (fi < 0) return;
    if (!isOpen(fi)) toggle(fi);
    target = fi;
    queueMicrotask(() => {
      let y = layout.offs[layout.rows.findIndex((r) => r.t === "file" && r.fi === fi)];
      if (line != null) {
        const j = layout.rows.findIndex((r) => r.fi === fi && newLine(r) === line);
        if (j >= 0) y = Math.max(y, layout.offs[j] - H.file - 6 * H.line);
      }
      if (scroller) scroller.scrollTo({ top: y, left: 0, behavior: Math.abs(scroller.scrollTop - y) < 4000 ? "smooth" : "auto" });
    });
  }
  function newLine(r: Row) {
    if (r.t === "line") return r.l.n;
    return r.t === "pair" && r.b != null ? files[r.fi].lines[r.b].n : null;
  }
  const SEV: Record<string, string> = { critical: "var(--red)", major: "var(--orange)", minor: "var(--yellow)" };
  const sevColor = (sev: string) => SEV[sev] ?? "var(--text3)";
  /** Color of the most severe finding anchored to a new-side line, or null. */
  function flag(fi: number, l: Line | null) {
    if (!l || l.n == null || l.k === "-") return null;
    const ms = markers.get(`${path(files[fi])}:${l.n}`);
    if (!ms?.length) return null;
    const rank = (sev: string) => ({ critical: 0, major: 1, minor: 2 })[sev] ?? 3;
    return sevColor(ms.reduce((a, b) => (rank(b.severity) < rank(a.severity) ? b : a)).severity);
  }
  const kind = (k: Line["k"]) => (k === "+" ? "a" : k === "-" ? "d" : k === "@" ? "h" : "c");
  const dir = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/") + 1) : "");
  const base = (p: string) => p.slice(p.lastIndexOf("/") + 1);
</script>

{#snippet header(fi: number)}
  {@const f = files[fi]}
  {@const open = isOpen(fi)}
  <button class="fh" onclick={() => toggle(fi)} title={open ? "Collapse file" : "Expand file"}>
    <Icon name="chevronRight" size={14} class="chev {open ? 'open' : ''}" />
    <span class="fpath">
      {#if f.renamed_file}<span class="dim">{f.old_path} → </span>{/if}<span class="dim">{dir(path(f))}</span><b>{base(path(f))}</b>
    </span>
    {#if f.new_file}<span class="pill success">new</span>{/if}
    {#if f.deleted_file}<span class="pill failed">deleted</span>{/if}
    {#if f.generated}<span class="pill">generated</span>{/if}
    {#if !open && autoCollapsed(f)}<span class="pill">{f.lines.length} lines hidden</span>{/if}
    <span class="stat"><span class="add">+{f.additions}</span><span class="del">−{f.deletions}</span></span>
  </button>
{/snippet}

{#snippet code(fi: number, li: number, l: Line)}
  {@const h = l.k === "@" || l.k === "\\" ? null : html(fi, li)}
  <!-- `h` is highlight.js output: text is escaped by hljs and splitLines rejects any other markup. -->
  {#if h !== null}<span class="code">{@html h}</span>{:else}<span class="code">{l.t}</span>{/if}
{/snippet}

{#snippet half(fi: number, li: number | null, side: "o" | "n")}
  {@const l = li == null ? null : files[fi].lines[li]}
  {@const fc = side === "n" ? flag(fi, l) : null}
  <div class="half k{l ? (l.k === ' ' ? 'c' : kind(l.k)) : 'e'}" class:flag={fc} style:--sc={fc}>
    <span class="gut"><span class="num">{l?.[side] ?? ""}</span><span class="sign">{l && l.k !== " " ? l.k : ""}</span></span>
    <span class="clip"><span class="shift">{#if l && li != null}{@render code(fi, li, l)}{/if}</span></span>
  </div>
{/snippet}

<div class="scroller" class:split bind:this={scroller} bind:clientHeight={viewH} bind:clientWidth={viewW} onscroll={onScroll} style:--vw="{viewW}px" style:--sx="{-sx}px">
  {#if top?.pinned}
    <div class="pin"><div class="pin-in" style:transform="translateY({top.shift}px)">{@render header(top.fi)}</div></div>
  {/if}
  <div class="inner" style:min-width={split ? null : `calc(${GUTTER}px + ${layout.maxLen}ch + 40px)`}>
    <div class="chm" bind:clientWidth={ch100}></div>
    <div style:height="{win.top}px"></div>
    {#each layout.rows.slice(win.start, win.end) as r, i (win.start + i)}
      {#if r.t === "file"}
        <div class="frow">{@render header(r.fi)}</div>
      {:else if r.t === "note"}
        <div class="note"><Icon name="alert" size={14} />{r.text}</div>
      {:else if r.t === "mark"}
        <button class="mark" style:--sc={sevColor(r.m.severity)} onclick={() => onmarker?.(r.m.id)} title="Show this finding">
          <Icon name="sparkles" size={13} class="mi" />
          <span class="ms">{r.m.severity}</span><span class="mt">{r.m.title}</span>
          <Icon name="chevronRight" size={13} class="mc" />
        </button>
      {:else if r.t === "gap"}
        <div class="gap"></div>
      {:else if r.t === "pair"}
        <div class="ln pair">{@render half(r.fi, r.a, "o")}{@render half(r.fi, r.b, "n")}</div>
      {:else}
        {@const fc = flag(r.fi, r.l)}
        <div class="ln k{kind(r.l.k)}" class:flag={fc} style:--sc={fc}>
          <span class="gut"><span class="num">{r.l.o ?? ""}</span><span class="num">{r.l.n ?? ""}</span><span class="sign">{r.l.k === "@" || r.l.k === " " ? "" : r.l.k}</span></span>
          {@render code(r.fi, r.li, r.l)}
        </div>
      {/if}
    {/each}
    <div style:height="{win.bottom}px"></div>
  </div>
</div>

<style>
  .scroller { overflow: auto; height: 100%; contain: strict; background: var(--bg1); overscroll-behavior: contain; }
  .inner { font: 12px/20px var(--mono); tab-size: 4; }
  /* Everything except code rows is pinned to the visible width so it never scrolls sideways. */
  .frow, .note, .mark, .pin { position: sticky; left: 0; width: var(--vw); }
  .pin { top: 0; height: 0; z-index: 4; }
  .pin-in { height: 36px; box-shadow: 0 6px 12px -8px rgba(0, 0, 0, .5); }
  .frow { height: 36px; }
  .fh {
    width: 100%; height: 36px; justify-content: flex-start; gap: 8px; border-radius: 0; border: none;
    border-top: 1px solid var(--border); border-bottom: 1px solid var(--border);
    background: var(--bg2); padding: 0 14px 0 10px; text-align: left; font: 500 12.5px/1 var(--mono);
  }
  .fh:hover:not(:disabled) { background: var(--bg3); }
  .fh:active:not(:disabled) { transform: none; }
  .fh :global(.chev) { color: var(--text3); transition: transform var(--t) var(--ease); }
  .fh :global(.chev.open) { transform: rotate(90deg); }
  .fpath { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fpath b { font-weight: 650; color: var(--text); }
  .fh .pill { font-family: -apple-system, system-ui, sans-serif; }
  .stat { display: flex; gap: 8px; font-size: 12px; }
  .add { color: var(--green); } .del { color: var(--red); }
  .note { height: 30px; display: flex; align-items: center; gap: 8px; padding: 0 14px; color: var(--text2); font: 12.5px -apple-system, system-ui, sans-serif; }
  .gap { height: 12px; }
  .ln { display: flex; height: 20px; white-space: pre; }
  .gut {
    position: sticky; left: 0; z-index: 1; display: flex; flex: none; width: 122px;
    background: var(--bg1); border-right: 1px solid var(--border); color: var(--text3);
    user-select: none; -webkit-user-select: none;
  }
  .num { width: 52px; text-align: right; padding-right: 10px; }
  .sign { width: 18px; text-align: center; }
  .code { padding: 0 24px 0 10px; }
  .ka { background: color-mix(in srgb, var(--green) 10%, var(--bg1)); }
  .ka .gut { background: color-mix(in srgb, var(--green) 20%, var(--bg1)); color: var(--green); }
  .kd { background: color-mix(in srgb, var(--red) 10%, var(--bg1)); }
  .kd .gut { background: color-mix(in srgb, var(--red) 20%, var(--bg1)); color: var(--red); }
  .kh { background: color-mix(in srgb, var(--accent) 7%, var(--bg1)); color: var(--text3); }
  .kh .gut { background: color-mix(in srgb, var(--accent) 10%, var(--bg1)); }
  .kh .code { font-style: italic; }
  /* Finding strip under its line (design B): one thin row tinted by severity; click opens it in the panel. */
  .mark {
    display: flex; justify-content: flex-start; gap: 8px; height: 24px; border-radius: 0; border: none;
    border-top: 1px solid color-mix(in srgb, var(--sc) 16%, transparent);
    border-bottom: 1px solid color-mix(in srgb, var(--sc) 16%, transparent);
    padding: 0 14px 0 132px; background: color-mix(in srgb, var(--sc) 6%, var(--bg1));
    font: 12px -apple-system, system-ui, sans-serif; white-space: nowrap; color: var(--text);
  }
  .mark:hover:not(:disabled) { background: color-mix(in srgb, var(--sc) 11%, var(--bg1)); }
  .mark:active:not(:disabled) { transform: none; }
  .mark :global(.mi) { color: var(--orange); flex: none; }
  .ms { font-size: 10px; font-weight: 700; letter-spacing: .04em; text-transform: uppercase; color: var(--sc); flex: none; }
  .mt { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; font-weight: 500; text-align: left; }
  .mark :global(.mc) { color: var(--text3); flex: none; transition: transform var(--t) var(--ease); }
  .mark:hover :global(.mc) { transform: translateX(2px); color: var(--text2); }
  /* Line a finding points at: soft severity tint + a bar on the gutter edge. */
  .flag { background: color-mix(in srgb, var(--sc) 9%, var(--bg1)); }
  .flag .gut { box-shadow: inset 3px 0 0 var(--sc); }
  .chm { position: absolute; width: 100ch; height: 0; visibility: hidden; }
  /* Split view: two fixed halves; code is clipped and shifted by --sx (see onWheel). */
  .split .ln { width: var(--vw); overflow: hidden; }
  .pair { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); }
  .half { display: flex; min-width: 0; }
  .half + .half { border-left: 1px solid var(--border); }
  .half .gut { position: static; width: 70px; }
  .clip { flex: 1; min-width: 0; overflow: hidden; }
  .shift { display: inline-block; transform: translateX(var(--sx)); }
  .ke { background: repeating-linear-gradient(135deg, transparent 0 6px, color-mix(in srgb, var(--text3) 8%, transparent) 6px 7px); }
  .ke .gut { background: var(--bg1); }
  .split .mark { padding-left: calc(var(--vw) / 2 + 80px); }
</style>
