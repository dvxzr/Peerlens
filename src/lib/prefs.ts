// Per-viewer UI conveniences (pane widths, visibility). Storage may be unavailable; never throw.
export function pref<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(`peerlens.${key}`);
    return v == null ? fallback : (JSON.parse(v) as T);
  } catch {
    return fallback;
  }
}
export function setPref(key: string, value: unknown) {
  try {
    localStorage.setItem(`peerlens.${key}`, JSON.stringify(value));
  } catch {
    /* ignore */
  }
}
