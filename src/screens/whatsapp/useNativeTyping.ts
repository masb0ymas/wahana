import { useEffect, useState } from "react";
import { nativeWa, onNativeMessages, onNativePresence, onNativeTyping } from "@/lib/nativeWa";
import { isGroup } from "@/lib/utils";

type Typist = { name: string | null; recording: boolean; at: number };
/** Whether the contact is online, and when they were last seen (unix ms, null when hidden). */
export type Presence = { online: boolean; lastSeen: number | null };

/** WhatsApp sends no "paused" when a phone drops off mid-sentence; a stale typing state lapses after this. */
const STALE_MS = 25_000;

/**
 * Who is typing in the open chat right now, and whether the contact is online. Only the open
 * chat is watched: the account goes online and subscribes to the contact's presence while it
 * is open, and stops on close.
 */
export function useNativeTyping(accountId: string, chatId: string, connected: boolean) {
  const [typists, setTypists] = useState<Record<string, Typist>>({});
  const [presence, setPresence] = useState<Presence | null>(null);

  useEffect(() => {
    setTypists({});
    setPresence(null);
    if (!connected) return;
    const online = onNativePresence((p) => {
      if (p.id !== accountId || !p.chatIds.includes(chatId)) return;
      // A presence without a last-seen keeps the one already known.
      setPresence((prev) => ({ online: p.online, lastSeen: p.lastSeen ?? prev?.lastSeen ?? null }));
      if (!p.online) setTypists({});
    });
    void nativeWa.watchTyping(accountId, chatId, true).catch(() => {});
    const typing = onNativeTyping((t) => {
      if (t.id !== accountId || !t.chatIds.includes(chatId)) return;
      setTypists((m) => {
        const next = { ...m };
        if (t.state === "paused") delete next[t.sender];
        else next[t.sender] = { name: t.senderName, recording: t.state === "recording", at: Date.now() };
        return next;
      });
    });
    // A message from them ends their typing, even if the "paused" never comes.
    const messages = onNativeMessages((batch) => {
      if (batch.id !== accountId) return;
      const arrived = batch.messages.filter((m) => m.chatId === chatId && !m.fromMe);
      if (!arrived.length) return;
      if (!isGroup(chatId)) return setTypists({});
      const names = new Set(arrived.map((x) => x.senderName));
      setTypists((m) => Object.fromEntries(Object.entries(m).filter(([, t]) => !t.name || !names.has(t.name))));
    });
    const sweep = setInterval(() => {
      setTypists((m) => {
        const now = Date.now();
        const fresh = Object.entries(m).filter(([, t]) => now - t.at < STALE_MS);
        return fresh.length === Object.keys(m).length ? m : Object.fromEntries(fresh);
      });
    }, 5_000);
    return () => {
      clearInterval(sweep);
      void online.then((u) => u());
      void typing.then((u) => u());
      void messages.then((u) => u());
      void nativeWa.watchTyping(accountId, chatId, false).catch(() => {});
    };
  }, [accountId, chatId, connected]);

  return { typists, presence };
}
