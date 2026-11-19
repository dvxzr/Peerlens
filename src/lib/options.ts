import type { MenuItem } from "./types";

export const CLAUDE_MODELS: MenuItem<string>[] = [
  { value: "sonnet", label: "Sonnet", desc: "Balanced quality and speed — recommended", icon: "sparkles", color: "var(--orange)" },
  { value: "opus", label: "Opus", desc: "Most thorough; uses your limits fastest", icon: "sparkles", color: "var(--purple)" },
  { value: "haiku", label: "Haiku", desc: "Fastest and lightest on limits", icon: "sparkles", color: "var(--green)" },
  { value: "fable", label: "Fable", desc: "Model alias offered by your Claude CLI", icon: "sparkles", color: "var(--blue)" },
];

// Values accepted by `claude --effort` (checked against the CLI).
export const EFFORTS: MenuItem<string>[] = [
  { value: "", label: "Auto", desc: "Claude Code's default", icon: "gauge" },
  { value: "low", label: "Low", desc: "Fastest, fewest tokens", icon: "gauge", color: "var(--green)" },
  { value: "medium", label: "Medium", desc: "Balanced", icon: "gauge", color: "var(--blue)" },
  { value: "high", label: "High", desc: "Thinks longer — better for tricky changes", icon: "gauge", color: "var(--yellow)" },
  { value: "xhigh", label: "Extra high", desc: "Slower, deeper reasoning", icon: "gauge", color: "var(--orange)" },
  { value: "max", label: "Max", desc: "Deepest; slowest and heaviest on limits", icon: "gauge", color: "var(--red)" },
];

const EFFORT_INFO: Record<string, [string, string, string]> = {
  minimal: ["Minimal", "Fastest, almost no reasoning", "var(--green)"],
  low: ["Low", "Fast, light reasoning", "var(--green)"],
  medium: ["Medium", "Balanced", "var(--blue)"],
  high: ["High", "Thinks longer — better for tricky changes", "var(--yellow)"],
  xhigh: ["Extra high", "Slower, deeper reasoning", "var(--orange)"],
  max: ["Max", "Deepest; slowest and heaviest on limits", "var(--red)"],
};

/** Effort menu for Codex: only the levels the chosen model supports. */
export function codexEfforts(levels: string[], def: string): MenuItem<string>[] {
  return [
    { value: "", label: "Auto", desc: def ? `Model default (${def})` : "Model default", icon: "gauge" },
    ...levels.filter((l) => EFFORT_INFO[l]).map((l) => ({ value: l, label: EFFORT_INFO[l][0], desc: EFFORT_INFO[l][1], icon: "gauge" as const, color: EFFORT_INFO[l][2] })),
  ];
}

export const LANGUAGES = ["English", "Ukrainian", "Polish", "German", "Spanish", "French"];
