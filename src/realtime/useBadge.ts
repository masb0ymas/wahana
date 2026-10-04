import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { TrayIcon } from "@tauri-apps/api/tray";

/** One session's unread, used to describe the badge per account. */
export interface BadgeGroup {
  name: string;
  unread: number;
}

/**
 * Mirror the unread total onto the dock/taskbar badge and tray. The dock badge is a single number
 * (an OS limitation), so the per-session breakdown goes in the tray tooltip instead.
 */
export function useBadge(groups: BadgeGroup[]) {
  const unread = groups.reduce((sum, g) => sum + g.unread, 0);
  const detail = groups
    .filter((g) => g.unread > 0)
    .map((g) => `${g.name}: ${g.unread}`)
    .join(" · ");
  useEffect(() => {
    const win = getCurrentWindow();
    win.setBadgeCount(unread > 0 ? unread : undefined).catch(() => {});
    TrayIcon.getById("main")
      .then((tray) => {
        if (!tray) return;
        void tray.setTooltip(detail ? `Wahana — ${detail}` : "Wahana");
        // macOS shows a text label beside the tray icon; null does not clear it, so pass "".
        void tray.setTitle(unread > 0 ? String(unread) : "").catch(() => {});
      })
      .catch(() => {});
  }, [unread, detail]);
}
