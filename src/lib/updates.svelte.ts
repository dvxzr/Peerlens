import { api, errText } from "./api";
import type { UpdateInfo } from "./types";

// Update state shared by Settings and the sidebar badge. Checks run only when the user asks.
export const updates = $state({
  version: "",
  checking: false,
  checked: false,
  available: null as UpdateInfo | null,
  error: "",
  installing: false,
  progress: 0, // 0..1, -1 = size unknown
});

export async function loadVersion() {
  try { updates.version = await api.appVersion(); } catch { /* ignore */ }
}

export async function checkForUpdates() {
  updates.checking = true;
  updates.error = "";
  try {
    updates.available = await api.checkUpdate();
    updates.checked = true;
  } catch (e) {
    updates.error = errText(e);
  } finally {
    updates.checking = false;
  }
}

export async function installUpdate() {
  updates.installing = true;
  updates.error = "";
  updates.progress = 0;
  try {
    // On success the app restarts into the new version and this promise never settles.
    await api.installUpdate((p) => {
      if (p.type === "downloading") updates.progress = p.total ? p.done / p.total : -1;
      else updates.progress = 1;
    });
  } catch (e) {
    updates.error = errText(e);
    updates.installing = false;
  }
}
