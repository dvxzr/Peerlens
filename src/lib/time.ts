const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
const units: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 31536000], ["month", 2592000], ["day", 86400], ["hour", 3600], ["minute", 60],
];

// Pure; pass a ticking `now` where the label must stay fresh while the view sits idle.
export function ago(iso: string | number, now = Date.now()): string {
  const s = (new Date(iso).getTime() - now) / 1000;
  for (const [u, sec] of units) if (Math.abs(s) >= sec) return rtf.format(Math.round(s / sec), u);
  return "just now";
}

export const fmtDate = (iso: string) => new Date(iso).toLocaleString();
