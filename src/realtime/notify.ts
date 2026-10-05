import { invoke } from "@tauri-apps/api/core";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";

let granted: boolean | null = null;

async function ensurePermission() {
  if (granted === null) {
    granted = await isPermissionGranted();
    if (!granted) granted = (await requestPermission()) === "granted";
  }
  return granted;
}

/** The chat a notification points at, kept with it so a click can open that conversation. */
export interface NotifyTarget {
  accountId: string;
  chatId: string;
}

/**
 * Shows a desktop notification. A notification that points at a chat goes through the native
 * `notify_chat` command, which raises the window and emits `wahana:notification-click` on click
 * (the plugin's desktop transport has no click handling). Plain notices use the plugin directly.
 */
export async function notifyText(title: string, body: string, target?: NotifyTarget) {
  if (!(await ensurePermission())) return;
  const trimmed = body.slice(0, 200);
  if (target) {
    await invoke("notify_chat", { title, body: trimmed, accountId: target.accountId, chatId: target.chatId });
    return;
  }
  sendNotification({ title, body: trimmed });
}
