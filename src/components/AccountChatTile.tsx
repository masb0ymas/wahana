import { useEffect, useMemo, useState } from "react";
import { Search } from "lucide-react";
import { Avatar } from "@/components/ui";
import { AckIcon } from "@/components/MessageExtras";
import { nativeWa, type NativeAccount, type NativeChat, type NativeWaStatus } from "@/lib/nativeWa";
import { usePicture } from "@/screens/whatsapp/usePicture";
import { Conversation, sanitizeTabOrder, type Filter } from "@/screens/WhatsAppScreen";
import { useWhatsApp } from "@/store/whatsapp";
import { useSettings } from "@/store/settings";
import { cn, displayId, errMsg, formatTime, isChannel, isDirect, isGroup } from "@/lib/utils";
import { stripWaMarkdown } from "@/lib/waMarkdown";

const dotTone: Record<NativeWaStatus, string> = {
  working: "bg-wa",
  starting: "bg-amber-400",
  qr: "bg-sky-400",
  stopped: "bg-neutral-400",
  logged_out: "bg-red-500",
  failed: "bg-red-500",
};

const tileShell =
  "flex flex-col min-h-0 h-full rounded-xl border border-neutral-200 dark:border-neutral-800 bg-white dark:bg-neutral-900 overflow-hidden";

/** One account: its chat list, or an opened chat right in the tile (back returns to the list). */
export function AccountChatTile({ account }: { account: NativeAccount }) {
  const tick = useWhatsApp((s) => s.messageTick[account.id] ?? 0);
  const [chats, setChats] = useState<NativeChat[]>([]);
  const [q, setQ] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [openChatId, setOpenChatId] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  const savedTabOrder = useSettings((s) => s.chatTabOrder);
  const tabs = useMemo(() => sanitizeTabOrder(savedTabOrder), [savedTabOrder]);
  const connected = account.status === "working";
  // The account's own profile picture, looked up from its own JID.
  const selfPicture = usePicture(account.id, account.me?.id ?? "", connected && !!account.me);

  useEffect(() => {
    let cancelled = false;
    nativeWa
      .chats(account.id)
      .then((list) => !cancelled && setChats(list))
      .catch((e) => !cancelled && setError(errMsg(e)));
    return () => {
      cancelled = true;
    };
  }, [account.id, tick, account.unread]);

  const shown = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return chats.filter((c) => {
      if (filter === "unread" && c.unread === 0) return false;
      if (filter === "private" && !isDirect(c.id)) return false;
      if (filter === "groups" && !isGroup(c.id)) return false;
      if (filter === "community" && !c.community) return false;
      if (filter === "channels" && !isChannel(c.id)) return false;
      if (!needle) return true;
      return c.name.toLowerCase().includes(needle) || c.id.includes(needle) || (c.phone ?? "").includes(needle);
    });
  }, [chats, q, filter]);

  if (openChatId) {
    return (
      <div className={tileShell}>
        <div className="flex-1 min-h-0 flex">
          <Conversation
            key={`${account.id}:${openChatId}`}
            account={account}
            chatId={openChatId}
            chat={chats.find((c) => c.id === openChatId)}
            tick={tick}
            onOpenChat={(ids) => {
              const id = ids.find((i) => chats.some((c) => c.id === i));
              if (id) setOpenChatId(id);
            }}
            initialDraft={null}
            onDraftUsed={() => {}}
            onError={setError}
            onBack={() => setOpenChatId(null)}
            compact
          />
        </div>
      </div>
    );
  }

  return (
    <div className={tileShell}>
      <div className="shrink-0 flex items-center gap-2 px-3 py-2 border-b border-neutral-200 dark:border-neutral-800">
        <span className={cn("w-2 h-2 rounded-full shrink-0", dotTone[account.status])} title={account.status} />
        <Avatar src={selfPicture} name={account.me?.pushName ?? account.name} size={26} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium">{account.name}</div>
          {account.me?.pushName && <div className="truncate text-[11px] text-neutral-500">~{account.me.pushName}</div>}
        </div>
        {account.unread > 0 && (
          <span className="shrink-0 min-w-[18px] h-[18px] px-1 rounded-full bg-wa text-[10px] font-bold text-white grid place-items-center">
            {account.unread}
          </span>
        )}
      </div>

      {!connected ? (
        <div className="flex-1 grid place-items-center p-4 text-center text-xs text-neutral-500">
          {account.status === "starting" ? "Starting…" : account.status === "qr" ? "Waiting for QR scan" : "Not connected"}
        </div>
      ) : (
        <>
          <div className="shrink-0 px-2 py-2 space-y-1.5">
            <div className="relative">
              <Search size={13} className="absolute left-2.5 top-2 text-neutral-400" />
              <input
                value={q}
                onChange={(e) => setQ(e.target.value)}
                placeholder="Search chats"
                className="w-full rounded-lg bg-neutral-100 dark:bg-neutral-800 pl-7 pr-2 py-1 text-xs outline-none"
              />
            </div>
            <div className="flex flex-wrap gap-1">
              {tabs.map((f) => (
                <button
                  key={f}
                  type="button"
                  onClick={() => setFilter(f)}
                  className={cn(
                    "rounded-full px-2 py-0.5 text-[10px] font-medium capitalize whitespace-nowrap",
                    filter === f ? "bg-wa-dark text-white" : "bg-neutral-100 dark:bg-neutral-800 text-neutral-600 dark:text-neutral-300",
                  )}
                >
                  {f}
                </button>
              ))}
            </div>
          </div>
          <div className="flex-1 min-h-0 overflow-y-auto">
            {error && <div className="px-3 py-2 text-[11px] text-red-600 dark:text-red-400 selectable">{error}</div>}
            {shown.length === 0 ? (
              <div className="px-3 py-2 text-[11px] text-neutral-500">{chats.length ? "No chats match." : "No chats yet."}</div>
            ) : (
              shown.map((c) => (
                <TileRow key={c.id} accountId={account.id} chat={c} connected={connected} onOpen={() => setOpenChatId(c.id)} />
              ))
            )}
          </div>
        </>
      )}
    </div>
  );
}

function TileRow({ accountId, chat, connected, onOpen }: { accountId: string; chat: NativeChat; connected: boolean; onOpen: () => void }) {
  const picture = usePicture(accountId, chat.id, connected);
  const title = chat.name || displayId(chat.id);
  const body = stripWaMarkdown(chat.lastText);
  return (
    <button
      type="button"
      onClick={onOpen}
      className="w-full flex items-center gap-2 px-3 py-1.5 text-left hover:bg-neutral-100 dark:hover:bg-neutral-800"
    >
      <Avatar src={picture} name={title} size={32} />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1">
          <span className={cn("truncate text-xs", chat.unread > 0 ? "font-semibold" : "font-medium")}>{title}</span>
          {chat.lastTimestamp > 0 && (
            <span className="ml-auto shrink-0 text-[10px] text-neutral-400">{formatTime(Math.floor(chat.lastTimestamp / 1000))}</span>
          )}
        </div>
        <div className="flex items-center gap-1">
          <span className={cn("truncate text-[11px]", chat.unread > 0 ? "text-neutral-700 dark:text-neutral-200" : "text-neutral-500")}>
            {chat.lastFromMe && <AckIcon ack={chat.lastAck} className="inline mr-1 -mt-0.5" />}
            {body || (chat.lastFromMe ? "" : "📎 Media")}
          </span>
          {chat.unread > 0 && (
            <span className="ml-auto shrink-0 min-w-[16px] h-[16px] px-1 rounded-full bg-wa text-[9px] font-bold text-white grid place-items-center">
              {chat.unread}
            </span>
          )}
        </div>
      </div>
    </button>
  );
}
