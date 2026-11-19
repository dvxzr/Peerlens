// Small GitLab-flavoured Markdown parser producing plain data, rendered by Markdown.svelte with
// text nodes only (no {@html}), so comment content can never inject markup or script.

export type Inline =
  | { t: "text"; v: string }
  | { t: "code"; v: string }
  | { t: "strong" | "em" | "del"; c: Inline[] }
  | { t: "link"; href: string; c: Inline[] }
  | { t: "image"; href: string; alt: string }
  | { t: "br" };

export type Block =
  | { t: "p"; c: Inline[] }
  | { t: "h"; level: number; c: Inline[] }
  | { t: "code"; lang: string; v: string }
  | { t: "quote"; c: Block[] }
  | { t: "list"; ordered: boolean; items: { c: Inline[]; depth: number; task?: boolean; done?: boolean }[] }
  | { t: "table"; head: Inline[][]; rows: Inline[][][] }
  | { t: "hr" };

const EMOJI: Record<string, string> = {
  thumbsup: "👍", "+1": "👍", thumbsdown: "👎", "-1": "👎", smile: "😄", smiley: "😃", grinning: "😀", laughing: "😆",
  joy: "😂", rofl: "🤣", wink: "😉", blush: "😊", slight_smile: "🙂", upside_down: "🙃", sweat_smile: "😅", thinking: "🤔",
  neutral_face: "😐", confused: "😕", cry: "😢", sob: "😭", rage: "😡", scream: "😱", see_no_evil: "🙈", facepalm: "🤦",
  shrug: "🤷", heart: "❤️", tada: "🎉", rocket: "🚀", fire: "🔥", eyes: "👀", warning: "⚠️", white_check_mark: "✅",
  heavy_check_mark: "✔️", ballot_box_with_check: "☑️", x: "❌", negative_squared_cross_mark: "❎", red_circle: "🔴",
  green_circle: "🟢", yellow_circle: "🟡", large_blue_circle: "🔵", bug: "🐛", sparkles: "✨", pray: "🙏", clap: "👏",
  ok_hand: "👌", point_right: "👉", point_up: "☝️", raised_hands: "🙌", handshake: "🤝", muscle: "💪", bulb: "💡",
  memo: "📝", pencil2: "✏️", lock: "🔒", unlock: "🔓", key: "🔑", zap: "⚡", construction: "🚧", star: "⭐", "100": "💯",
  question: "❓", exclamation: "❗", information_source: "ℹ️", no_entry: "⛔", stop_sign: "🛑", boom: "💥", hammer: "🔨",
  wrench: "🔧", gear: "⚙️", package: "📦", link: "🔗", mag: "🔍", recycle: "♻️", arrow_right: "➡️", arrow_left: "⬅️",
  arrow_up: "⬆️", arrow_down: "⬇️", hourglass: "⌛", hourglass_flowing_sand: "⏳", calendar: "📅", chart_with_upwards_trend: "📈",
  art: "🎨", books: "📚", coffee: "☕", beers: "🍻", robot: "🤖", skull: "💀", poop: "💩", heavy_plus_sign: "➕",
  heavy_minus_sign: "➖", speech_balloon: "💬", bell: "🔔", pushpin: "📌", paperclip: "📎", trophy: "🏆", medal: "🏅",
  ghost: "👻", rotating_light: "🚨", checkered_flag: "🏁", white_circle: "⚪", black_circle: "⚫", new: "🆕", ok: "🆗",
};

const SAFE_URL = /^https?:\/\//i;
export const safeUrl = (u: string) => (SAFE_URL.test(u.trim()) ? u.trim() : null);

// Order matters: code first so nothing inside backticks is parsed.
const INLINE = new RegExp(
  [
    "(`+)([\\s\\S]*?[^`])\\1(?!`)", // 1,2 code
    "!\\[([^\\]]*)\\]\\(\\s*<?([^)\\s>]+)>?(?:\\s+[\"'][^)]*[\"'])?\\s*\\)", // 3,4 image
    "\\[((?:[^\\[\\]]|\\[[^\\]]*\\])+)\\]\\(\\s*<?([^)\\s>]+)>?(?:\\s+[\"'][^)]*[\"'])?\\s*\\)", // 5,6 link
    "<(https?://[^>\\s]+)>", // 7 autolink
    "(https?://[^\\s<>()\\]]+[^\\s<>()\\].,;:!?'\"])", // 8 bare url
    "\\*\\*([^*\\n]+?)\\*\\*|__([^_\\n]+?)__", // 9,10 strong
    "~~([^~\\n]+?)~~", // 11 del
    "(?<![\\w*])\\*(?![\\s*])([^*\\n]+?)(?<!\\s)\\*(?![\\w*])|(?<![\\w_])_(?![\\s_])([^_\\n]+?)(?<!\\s)_(?![\\w_])", // 12,13 em
    ":([a-z0-9_+-]{1,40}):", // 14 emoji
    "<br\\s*/?>", // line break tag
  ].join("|"),
  "gi",
);

export function inline(src: string): Inline[] {
  const out: Inline[] = [];
  const text = (v: string) => {
    if (!v) return;
    const last = out[out.length - 1];
    if (last?.t === "text") last.v += v; else out.push({ t: "text", v });
  };
  let i = 0;
  for (const m of src.matchAll(INLINE)) {
    text(src.slice(i, m.index));
    i = m.index! + m[0].length;
    if (m[2] !== undefined) out.push({ t: "code", v: m[2].replace(/^ (.*) $/, "$1") });
    else if (m[4] !== undefined) {
      const href = safeUrl(m[4]);
      href ? out.push({ t: "image", href, alt: m[3] }) : text(m[3] || "image");
    } else if (m[6] !== undefined) {
      const href = safeUrl(m[6]);
      href ? out.push({ t: "link", href, c: inline(m[5]) }) : out.push(...inline(m[5]));
    } else if (m[7] !== undefined || m[8] !== undefined) {
      const u = (m[7] ?? m[8])!;
      out.push({ t: "link", href: u, c: [{ t: "text", v: u }] });
    } else if (m[9] !== undefined || m[10] !== undefined) out.push({ t: "strong", c: inline((m[9] ?? m[10])!) });
    else if (m[11] !== undefined) out.push({ t: "del", c: inline(m[11]) });
    else if (m[12] !== undefined || m[13] !== undefined) out.push({ t: "em", c: inline((m[12] ?? m[13])!) });
    else if (m[14] !== undefined) text(EMOJI[m[14].toLowerCase()] ?? m[0]);
    else out.push({ t: "br" });
  }
  text(src.slice(i));
  return out;
}

// Raw HTML is not rendered: keep the text of harmless tags, drop the tags themselves.
const stripTags = (s: string) =>
  s.replace(/<br\s*\/?>|<\/summary>/gi, "\n").replace(/<\/?(?:details|summary|div|span|p|b|i|em|strong|sub|sup|small|kbd|center|font|img)[^>]*>/gi, "");

const cells = (line: string) => line.trim().replace(/^\|/, "").replace(/\|$/, "").split(/(?<!\\)\|/).map((c) => inline(c.trim()));

export function parse(src: string): Block[] {
  const lines = src.replace(/\r\n?/g, "\n").split("\n");
  const out: Block[] = [];
  let para: string[] = [];
  const flush = () => {
    if (!para.length) return;
    const c: Inline[] = [];
    para.forEach((l, k) => {
      if (k) c.push({ t: "br" });
      c.push(...inline(stripTags(l)));
    });
    out.push({ t: "p", c });
    para = [];
  };
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    let m: RegExpMatchArray | null;
    if ((m = line.match(/^\s*(`{3,}|~{3,})\s*([\w+#.-]*)/))) {
      flush();
      const fence = m[1], body: string[] = [];
      while (++i < lines.length && !lines[i].trim().startsWith(fence)) body.push(lines[i]);
      out.push({ t: "code", lang: m[2], v: body.join("\n") });
    } else if (!line.trim()) flush();
    else if ((m = line.match(/^\s{0,3}(#{1,6})\s+(.*?)\s*#*\s*$/))) {
      flush();
      out.push({ t: "h", level: m[1].length, c: inline(stripTags(m[2])) });
    } else if (/^\s{0,3}([-*_])(\s*\1){2,}\s*$/.test(line)) {
      flush();
      out.push({ t: "hr" });
    } else if (/^\s{0,3}>/.test(line)) {
      flush();
      const body: string[] = [];
      while (i < lines.length && /^\s{0,3}>/.test(lines[i])) body.push(lines[i++].replace(/^\s{0,3}>\s?/, ""));
      i--;
      out.push({ t: "quote", c: parse(body.join("\n")) });
    } else if (/^\s*([-*+]|\d+[.)])\s+/.test(line)) {
      flush();
      const ordered = /^\s*\d/.test(line);
      const items: { c: Inline[]; depth: number; task?: boolean; done?: boolean }[] = [];
      while (i < lines.length && (m = lines[i].match(/^(\s*)([-*+]|\d+[.)])\s+(.*)$/))) {
        // A top-level item of the other kind (bullet vs number) starts a new list.
        if (items.length && !m[1] && /^\d/.test(m[2]) !== ordered) break;
        let body = m[3];
        const task = body.match(/^\[([ xX])\]\s+/);
        if (task) body = body.slice(task[0].length);
        items.push({ c: inline(stripTags(body)), depth: Math.min(4, Math.floor(m[1].replace(/\t/g, "  ").length / 2)), task: !!task, done: !!task && task[1] !== " " });
        i++;
      }
      i--;
      out.push({ t: "list", ordered, items });
    } else if (line.includes("|") && /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/.test(lines[i + 1] ?? "")) {
      flush();
      const head = cells(line), rows: Inline[][][] = [];
      i += 2;
      while (i < lines.length && lines[i].includes("|") && lines[i].trim()) rows.push(cells(lines[i++]));
      i--;
      out.push({ t: "table", head, rows });
    } else para.push(line);
  }
  flush();
  return out;
}
