import { useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
import { ResizeHandle, usePaneWidth } from "@/components/ResizeHandle";
import { errMsg } from "@/lib/utils";
import { nativeWa, type NativeAccount, type NativeChat } from "@/lib/nativeWa";
import { nativeChatKey } from "@/lib/account";
import { Pairing } from "@/screens/whatsapp/Pairing";
import { MUTE_FOREVER, isMutedUntil, useChatPrefs } from "@/store/chatPrefs";
import { useWhatsApp } from "@/store/whatsapp";
import { type CrossReply } from "@/screens/whatsapp/shared";
import { ChatList } from "@/screens/whatsapp/ChatList";
import { Conversation } from "@/screens/whatsapp/Conversation";
import { pickChat } from "@/screens/whatsapp/ChatDialogs";

/**
 * Chat screen for a native WhatsApp account (the Rust client, no embedded WhatsApp Web).
 * Pairing happens in place: while the account waits for a scan, the conversation area
 * shows the QR code.
 *
 * History comes from the local store (see `whatsapp_db/`); attachments download on
 * demand; the AI tools live in `whatsapp/NativeAi`.
 */

export function WhatsAppScreen({
  account,
  header,
  embedded,
}: {
  account: NativeAccount;
  header: React.ReactNode;
  /** One self-contained cell: the chat list and the conversation swap in place, with a back
   * button, instead of the side-by-side panes with a resize handle. Used by the grid view. */
  embedded?: boolean;
}) {
  const tick = useWhatsApp((s) => s.messageTick[account.id] ?? 0);
  const setOpenChat = useWhatsApp((s) => s.setOpenChat);
  const pendingOpen = useWhatsApp((s) => s.pendingOpen);
  const clearPendingOpen = useWhatsApp((s) => s.clearPendingOpen);
  const [listWidth, setListWidth] = usePaneWidth("chatList", 320, 240, 560);
  const [chats, setChats] = useState<NativeChat[]>([]);
  const [chatId, setChatId] = useState<string | null>(null);
  const [pendingReply, setPendingReply] = useState<CrossReply | null>(null);
  /** A message id to scroll to once the chat opened by a cross-chat quote is mounted. */
  const [pendingJump, setPendingJump] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // A chat id belongs to one account: switching accounts drops the selection.
  useEffect(() => {
    setChatId(null);
    setError(null);
  }, [account.id]);

  // A notification click names a chat: open it once that account is the one on screen.
  // Declared after the reset above so it wins on the render that switches accounts.
  useEffect(() => {
    if (!pendingOpen || pendingOpen.accountId !== account.id) return;
    setChatId(pendingOpen.chatId);
    clearPendingOpen();
  }, [pendingOpen, account.id, clearPendingOpen]);

  useEffect(() => {
    setOpenChat(account.id, chatId);
    return () => setOpenChat(account.id, null);
  }, [account.id, chatId, setOpenChat]);

  useEffect(() => {
    let cancelled = false;
    nativeWa
      .chats(account.id)
      .then((list) => !cancelled && setChats(list))
      .catch((e) => setError(errMsg(e)));
    return () => {
      cancelled = true;
    };
  }, [account.id, tick, account.unread]);

  // Pins made or removed on the phone (or another linked device) show here too. A chat with no
  // WhatsApp record keeps whatever this app has, so pins kept only here (past WhatsApp's three)
  // are left alone.
  useEffect(() => {
    const prefs = useChatPrefs.getState();
    for (const c of chats) {
      if (c.pinnedAt == null) continue;
      const key = nativeChatKey(account.id, c.id);
      const local = prefs.pinned[key];
      if (c.pinnedAt > 0 && local === undefined) prefs.setPinned(key, c.pinnedAt);
      else if (c.pinnedAt === 0 && local !== undefined) prefs.setPinned(key, null);
    }
  }, [account.id, chats]);

  // Mute state set on the phone (or another linked device) wins over the local copy.
  useEffect(() => {
    const prefs = useChatPrefs.getState();
    for (const c of chats) {
      if (c.mutedUntil == null) continue;
      const key = nativeChatKey(account.id, c.id);
      const local = prefs.muted[key];
      if (c.mutedUntil === 0) {
        if (isMutedUntil(local)) prefs.setMuted(key, null);
      } else if (local !== c.mutedUntil && !(c.mutedUntil === MUTE_FOREVER && isMutedUntil(local) && local <= 1)) {
        prefs.setMuted(key, c.mutedUntil);
      }
    }
  }, [account.id, chats]);

  const chat = chats.find((c) => c.id === chatId);

  if (embedded) {
    return (
      <div className="flex-1 min-h-0 flex flex-col">
        {account.status === "qr" ? (
          <div className="flex-1 min-h-0 overflow-y-auto">
            <Pairing accountId={account.id} />
          </div>
        ) : chatId ? (
          <>
            {(error || account.error) && (
              <div className="shrink-0 px-3 py-1.5 text-[11px] bg-red-50 dark:bg-red-950/40 text-red-700 dark:text-red-300 selectable">
                {error || account.error}
              </div>
            )}
            <div className="flex-1 min-h-0 flex relative">
              <Conversation
                key={`${account.id}:${chatId}`}
                account={account}
                chatId={chatId}
                chat={chat}
                tick={tick}
                onOpenChat={(ids, reply, jump) => {
                  setPendingReply(reply ?? null);
                  setPendingJump(jump ?? null);
                  setChatId(pickChat(chats, ids));
                }}
                initialReply={pendingReply}
                onReplyUsed={() => setPendingReply(null)}
                initialJump={pendingJump}
                onJumpUsed={() => setPendingJump(null)}
                onError={setError}
                onBack={() => setChatId(null)}
                compact
                infoOverlay
              />
            </div>
          </>
        ) : (
          <ChatList
            account={account}
            header={null}
            chats={chats}
            selected={chatId}
            onSelect={setChatId}
            onDeleted={(ids) => ids.includes(chatId ?? "") && setChatId(null)}
            error={error}
            fill
          />
        )}
      </div>
    );
  }

  return (
    <>
      <ChatList
        account={account}
        header={header}
        chats={chats}
        selected={chatId}
        onSelect={setChatId}
        onDeleted={(ids) => ids.includes(chatId ?? "") && setChatId(null)}
        error={error}
        width={listWidth}
      />
      <ResizeHandle onDrag={(dx) => setListWidth((w) => w + dx)} onReset={() => setListWidth(320)} />
      {account.status === "qr" ? (
        <Pairing accountId={account.id} />
      ) : chatId ? (
        <Conversation
          key={`${account.id}:${chatId}`}
          account={account}
          chatId={chatId}
          chat={chat}
          tick={tick}
          onOpenChat={(ids, reply, jump) => {
            setPendingReply(reply ?? null);
            setPendingJump(jump ?? null);
            setChatId(pickChat(chats, ids));
          }}
          initialReply={pendingReply}
          onReplyUsed={() => setPendingReply(null)}
          initialJump={pendingJump}
          onJumpUsed={() => setPendingJump(null)}
          onError={setError}
        />
      ) : (
        <Empty>{account.status === "starting" ? <Loader2 className="animate-spin" /> : "Select a chat"}</Empty>
      )}
    </>
  );
}

function Empty({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex-1 grid place-items-center text-neutral-500 text-sm">
      <div className="flex flex-col items-center gap-3">{children}</div>
    </div>
  );
}

// ── Chat list ────────────────────────────────────────────────────────────
