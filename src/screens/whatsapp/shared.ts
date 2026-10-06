/** Constants, types and helpers shared by the native WhatsApp chat screen's parts. */
import { type NativeMessage } from "@/lib/nativeWa";

/** Timestamps from the backend are milliseconds; the shared formatters take seconds. */
export const secs = (ms: number) => Math.floor(ms / 1000);

/** Chats WhatsApp lets you pin; more are kept only in this app. */
export const WA_PIN_LIMIT = 3;

export type Filter = "all" | "unread" | "private" | "groups" | "community" | "channels" | "archived";

/** Every filter tab, in the default order. */
export const KNOWN_FILTERS: Filter[] = ["all", "unread", "private", "groups", "community", "channels", "archived"];

/**
 * Keeps a stored tab order usable: unknown ids are dropped and missing known filters are
 * inserted at their default position, so an order saved before a tab existed (or a custom
 * one) gains the new tabs without losing the user's arrangement.
 */
export function sanitizeTabOrder(order: string[]): Filter[] {
  const known = [...new Set(order.filter((f): f is Filter => (KNOWN_FILTERS as string[]).includes(f)))];
  for (const f of KNOWN_FILTERS) {
    if (known.includes(f)) continue;
    // Insert before the first known filter that follows it by default, else append.
    const after = KNOWN_FILTERS.slice(KNOWN_FILTERS.indexOf(f) + 1).find((x) => known.includes(x));
    known.splice(after ? known.indexOf(after) : known.length, 0, f);
  }
  return known;
}

/** WhatsApp only accepts edits within this long after sending. */
export const EDIT_WINDOW_MS = 15 * 60 * 1000;

/** Messages read from the local history per page. */
export const PAGE = 100;

/**
 * A reply that quotes a message from another chat, opened by "Reply privately": the message, the
 * chat that stores it, and that chat's name for the composer's reply bar.
 */
export type CrossReply = { message: NativeMessage; chatId: string; chatName: string };

/** Pasted, dropped or picked file waiting to be sent, with the draft as its caption. */
export interface Attachment {
  file: File;
  preview: string | null;
}
