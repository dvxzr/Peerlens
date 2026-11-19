import hljs from "highlight.js/lib/core";
import type { FileDiff } from "./types";

// Each language is a separate lazily loaded chunk.
const loaders: Record<string, () => Promise<{ default: any }>> = {
  python: () => import("highlight.js/lib/languages/python"),
  rust: () => import("highlight.js/lib/languages/rust"),
  typescript: () => import("highlight.js/lib/languages/typescript"),
  javascript: () => import("highlight.js/lib/languages/javascript"),
  json: () => import("highlight.js/lib/languages/json"),
  yaml: () => import("highlight.js/lib/languages/yaml"),
  ini: () => import("highlight.js/lib/languages/ini"),
  go: () => import("highlight.js/lib/languages/go"),
  java: () => import("highlight.js/lib/languages/java"),
  kotlin: () => import("highlight.js/lib/languages/kotlin"),
  swift: () => import("highlight.js/lib/languages/swift"),
  c: () => import("highlight.js/lib/languages/c"),
  cpp: () => import("highlight.js/lib/languages/cpp"),
  csharp: () => import("highlight.js/lib/languages/csharp"),
  ruby: () => import("highlight.js/lib/languages/ruby"),
  php: () => import("highlight.js/lib/languages/php"),
  bash: () => import("highlight.js/lib/languages/bash"),
  sql: () => import("highlight.js/lib/languages/sql"),
  markdown: () => import("highlight.js/lib/languages/markdown"),
  xml: () => import("highlight.js/lib/languages/xml"),
  css: () => import("highlight.js/lib/languages/css"),
  scss: () => import("highlight.js/lib/languages/scss"),
  dockerfile: () => import("highlight.js/lib/languages/dockerfile"),
  makefile: () => import("highlight.js/lib/languages/makefile"),
  protobuf: () => import("highlight.js/lib/languages/protobuf"),
  scala: () => import("highlight.js/lib/languages/scala"),
  dart: () => import("highlight.js/lib/languages/dart"),
  lua: () => import("highlight.js/lib/languages/lua"),
  elixir: () => import("highlight.js/lib/languages/elixir"),
  graphql: () => import("highlight.js/lib/languages/graphql"),
};

const byExt: Record<string, string> = {
  py: "python", pyi: "python", rs: "rust", ts: "typescript", tsx: "typescript", mts: "typescript", cts: "typescript",
  js: "javascript", jsx: "javascript", mjs: "javascript", cjs: "javascript", json: "json", jsonc: "json",
  yml: "yaml", yaml: "yaml", toml: "ini", ini: "ini", cfg: "ini", conf: "ini", env: "bash", go: "go", java: "java",
  kt: "kotlin", kts: "kotlin", swift: "swift", c: "c", h: "c", cc: "cpp", cpp: "cpp", cxx: "cpp", hpp: "cpp",
  cs: "csharp", rb: "ruby", php: "php", sh: "bash", bash: "bash", zsh: "bash", sql: "sql", md: "markdown",
  html: "xml", htm: "xml", xml: "xml", svg: "xml", svelte: "xml", vue: "xml", css: "css", scss: "scss",
  proto: "protobuf", scala: "scala", dart: "dart", lua: "lua", ex: "elixir", exs: "elixir", graphql: "graphql", gql: "graphql",
};

export function langFor(path: string): string | null {
  const name = path.split("/").pop()!.toLowerCase();
  if (name === "dockerfile" || name.startsWith("dockerfile.")) return "dockerfile";
  if (name === "makefile") return "makefile";
  const ext = name.includes(".") ? name.split(".").pop()! : "";
  return byExt[ext] ?? null;
}

const loaded = new Map<string, Promise<boolean>>();
function ensure(lang: string): Promise<boolean> {
  let p = loaded.get(lang);
  if (!p) {
    p = loaders[lang]().then((m) => (hljs.registerLanguage(lang, m.default), true)).catch(() => false);
    loaded.set(lang, p);
  }
  return p;
}

/** Split highlight.js HTML into lines, closing and reopening spans at line breaks.
 *  Returns null if the markup contains anything other than hljs spans and escaped text. */
export function splitLines(html: string): string[] | null {
  const out: string[] = [];
  const stack: string[] = [];
  let cur = "";
  let consumed = 0;
  for (const m of html.matchAll(/<span class="[\w\- ]*">|<\/span>|\n|[^<\n]+/g)) {
    const t = m[0];
    consumed += t.length;
    if (t === "\n") {
      out.push(cur + "</span>".repeat(stack.length));
      cur = stack.join("");
    } else {
      if (t.startsWith("<span")) stack.push(t);
      else if (t === "</span>") stack.pop();
      cur += t;
    }
  }
  if (consumed !== html.length) return null;
  out.push(cur + "</span>".repeat(stack.length));
  return out;
}

const MAX_LINES = 6000;
const cache = new WeakMap<FileDiff, (string | null)[] | null>();
const pending = new WeakSet<FileDiff>();

export const cached = (f: FileDiff) => cache.get(f);

/** Highlight a file's diff lines (old and new side separately). Resolves true when the cache changed. */
export async function highlightFile(f: FileDiff): Promise<boolean> {
  if (cache.has(f) || pending.has(f)) return false;
  const lang = langFor(f.new_path || f.old_path);
  if (!lang || f.lines.length > MAX_LINES || !(await ensure(lang))) {
    cache.set(f, null);
    return false;
  }
  pending.add(f);
  const side = (keep: (k: string) => boolean) => {
    const idx: number[] = [];
    const text: string[] = [];
    f.lines.forEach((l, i) => keep(l.k) && (idx.push(i), text.push(l.t)));
    const html = splitLines(hljs.highlight(text.join("\n"), { language: lang, ignoreIllegals: true }).value);
    return { idx, html };
  };
  const res: (string | null)[] = new Array(f.lines.length).fill(null);
  for (const s of [side((k) => k === " " || k === "+"), side((k) => k === "-")]) {
    if (!s.html) continue;
    s.idx.forEach((li, j) => (res[li] = s.html![j] ?? null));
  }
  cache.set(f, res);
  pending.delete(f);
  return true;
}
