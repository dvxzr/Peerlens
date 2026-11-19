// macOS: overlay title bar with traffic lights and ⌘ shortcuts; Linux/others: native title bar and Ctrl.
export const isMac = /Mac/i.test(navigator.userAgent);

/** Shortcut label, e.g. kb("R") → "⌘R" on macOS, "Ctrl+R" elsewhere. */
export const kb = (key: string) => (isMac ? `⌘${key}` : `Ctrl+${key === "↵" ? "Enter" : key}`);

// Lets CSS reserve space for the traffic lights only where they exist.
document.documentElement.classList.toggle("mac", isMac);
