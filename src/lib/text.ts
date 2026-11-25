/** Pasted text (e.g. a Jira task) without the whitespace that eats a char limit: common indentation,
 * trailing spaces, runs of spaces inside lines and runs of blank lines. Relative indentation is kept. */
export function squeeze(t: string): string {
  const lines = t
    .replace(/\r\n?/g, "\n")
    .replace(/[   ]/g, " ")
    .replace(/[​﻿]/g, "")
    .split("\n")
    .map((l) => l.replace(/\s+$/, ""));
  const indents = lines.filter(Boolean).map((l) => /^[ \t]*/.exec(l)![0].length);
  const cut = indents.length ? Math.min(...indents) : 0;
  return lines
    .map((l) => l.slice(cut).replace(/(\S)[ \t]{2,}/g, "$1 "))
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
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
