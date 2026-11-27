/** Pasted text (e.g. a Jira task) without the whitespace that eats a char limit: indentation is cut to
 * 2 spaces per nesting level, runs of spaces become one, and blank lines go (or shrink to one between
 * paragraphs when the text is not double-spaced). */
export function squeeze(t: string): string {
  const lines = t
    .replace(/\r\n?/g, "\n")
    .replace(/[\u200b-\u200d\u2060\ufeff]/g, "")
    .replace(/[^\S\n]/g, " ") // tabs, no-break and other Unicode spaces
    .split("\n")
    .map((l) => l.trimEnd());
  const indent = (l: string) => l.length - l.trimStart().length;
  // Each distinct indent width becomes one 2-space level: nesting is kept, the width is not.
  const widths = [...new Set(lines.filter(Boolean).map(indent))].sort((a, b) => a - b);
  const text = lines
    .map((l) => l && "  ".repeat(widths.indexOf(indent(l))) + l.trimStart().replace(/ {2,}/g, " "))
    .join("\n");
  // Copied rich text often puts a blank line after every line; then blank lines carry no structure.
  const core = text.trim();
  const gaps = core.match(/\n\n+/g)?.length ?? 0;
  const doubleSpaced = gaps >= (lines.filter(Boolean).length - 1) / 2;
  return core.replace(doubleSpaced ? /\n\n+/g : /\n{3,}/g, doubleSpaced ? "\n" : "\n\n");
}

/** onpaste for a textarea: inserts the squeezed text; plain paste when there is nothing to squeeze. */
export function pasteSqueezed(e: ClipboardEvent) {
  const raw = e.clipboardData?.getData("text/plain");
  if (!raw) return;
  const t = squeeze(raw);
  if (t === raw) return;
  e.preventDefault();
  // insertText keeps native undo and fires `input`, so bound values and counters update.
  if (document.execCommand("insertText", false, t)) return;
  const el = e.currentTarget as HTMLTextAreaElement;
  el.setRangeText(t, el.selectionStart, el.selectionEnd, "end");
  el.dispatchEvent(new Event("input", { bubbles: true }));
}

/** Avatar letters for a GitLab user: "Arsen Pidhoretskyi" → "AP". */
export const initials = (u: { name: string; username: string }) =>
  (u.name || u.username).trim().split(/\s+/).slice(0, 2).map((w) => w[0]?.toUpperCase() ?? "").join("");
/** A stable per-user avatar color. */
export const avatarColor = (username: string) => `hsl(${[...username].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7)} 55% 62%)`;

const SEVERITIES = ["critical", "major", "minor", "nit"];
/** The most severe of the given finding severities ("nit" when none is known). */
export const worstSeverity = (sevs: string[]) => SEVERITIES.find((s) => sevs.includes(s)) ?? "nit";

/** Code without the indentation all its lines share, for display (a snippet cut from a nested block). */
export function dedent(code: string): string {
  const lines = code.replace(/\s+$/, "").split("\n");
  const pad = Math.min(...lines.filter((l) => l.trim()).map((l) => l.length - l.trimStart().length));
  return Number.isFinite(pad) && pad > 0 ? lines.map((l) => l.slice(pad)).join("\n") : lines.join("\n");
}
