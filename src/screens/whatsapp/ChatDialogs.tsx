/** Dialogs for starting a chat and forwarding a message. */
import { useEffect, useState } from "react";
import { Loader2, Search, X, Plus } from "lucide-react";
import { Avatar, Button, Input } from "@/components/ui";
import { displayId, errMsg, isChannel } from "@/lib/utils";
import { nativeWa, type NativeChat, type NativeMessage } from "@/lib/nativeWa";

/** Start a chat: pick a chat you already have, or type a phone number with its country code (no "+" needed). */
/** The chat to open for `ids`: the first one already known, else one known under the same
 * phone number (it may live under its privacy id, @lid), else the last id as a new chat. */
export function pickChat(chats: NativeChat[], ids: string[]): string {
  const known = ids.find((id) => chats.some((c) => c.id === id));
  if (known) return known;
  for (const id of ids) {
    if (!id.endsWith("@s.whatsapp.net")) continue;
    const digits = id.split("@")[0];
    const byPhone = chats.find((c) => c.phone && c.phone.replace(/\D/g, "") === digits);
    if (byPhone) return byPhone.id;
  }
  return ids[ids.length - 1]!;
}

export function NewChatDialog({ chats, onOpen, onClose }: { chats: NativeChat[]; onOpen: (id: string) => void; onClose: () => void }) {
  const [value, setValue] = useState("");
  const digits = value.replace(/\D/g, "");
  const term = value.trim().toLowerCase();
  const contacts = chats
    .filter((c) => c.id !== "status@broadcast" && !isChannel(c.id))
    .filter((c) => !term || (c.name ?? "").toLowerCase().includes(term) || (c.phone ?? "").includes(term) || c.id.includes(term))
    .slice(0, 50);
  // A chat we already know for this number may live under its privacy id (@lid).
  const known =
    digits.length >= 8
      ? chats.find((c) => c.id === `${digits}@s.whatsapp.net` || (c.phone && c.phone.replace(/\D/g, "") === digits))
      : undefined;
  const start = (id: string) => {
    onOpen(id);
    onClose();
  };
  const startNumber = () => {
    if (digits.length >= 8) start(known?.id ?? `${digits}@s.whatsapp.net`);
  };
  return (
    <div
      className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-4"
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <form
        onSubmit={(e) => {
          e.preventDefault();
          startNumber();
        }}
        className="w-full max-w-[380px] max-h-full flex flex-col rounded-xl bg-white dark:bg-neutral-900 shadow-2xl"
      >
        <div className="flex items-center gap-2 p-3 border-b border-neutral-200 dark:border-neutral-800">
          <span className="font-semibold flex-1">Start chat</span>
          <button type="button" onClick={onClose}>
            <X size={16} />
          </button>
        </div>
        <div className="p-2 relative">
          <Search size={14} className="absolute left-4 top-4.5 text-neutral-400" />
          <Input
            className="pl-8"
            inputMode="tel"
            placeholder="Search or type a number"
            value={value}
            onChange={(e) => setValue(e.target.value)}
            autoFocus
          />
        </div>
        <div className="flex-1 overflow-y-auto">
          {digits.length >= 8 && !known && (
            <button
              type="button"
              onClick={startNumber}
              className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm hover:bg-neutral-100 dark:hover:bg-neutral-800"
            >
              <span className="grid h-[30px] w-[30px] shrink-0 place-items-center rounded-full bg-wa/15 text-wa-dark dark:text-wa">
                <Plus size={14} />
              </span>
              <span className="min-w-0 flex-1">
                <span className="block truncate">Start chat</span>
                <span className="block text-xs text-neutral-500">+{digits}</span>
              </span>
            </button>
          )}
          {contacts.map((c) => {
            const name = c.name || displayId(c.id);
            return (
              <button
                key={c.id}
                type="button"
                onClick={() => start(c.id)}
                className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm hover:bg-neutral-100 dark:hover:bg-neutral-800"
              >
                <Avatar name={name} size={30} />
                <span className="min-w-0 flex-1">
                  <span className="block truncate">{name}</span>
                  {c.phone && <span className="block text-xs text-neutral-500">{c.phone}</span>}
                </span>
              </button>
            );
          })}
          {contacts.length === 0 && digits.length < 8 && <p className="px-3 py-2 text-xs text-neutral-500">No chats found.</p>}
        </div>
        <div className="flex justify-end gap-2 p-3 border-t border-neutral-200 dark:border-neutral-800">
          <Button type="button" variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={digits.length < 8}>
            Start chat
          </Button>
        </div>
      </form>
    </div>
  );
}

/** Forward a stored message to another chat of the same account. */
export function NativeForwardDialog({
  accountId,
  fromChatId,
  message,
  onClose,
  onError,
}: {
  accountId: string;
  fromChatId: string;
  message: NativeMessage;
  onClose: () => void;
  onError: (e: string) => void;
}) {
  const [chats, setChats] = useState<NativeChat[] | null>(null);
  const [q, setQ] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [done, setDone] = useState<Set<string>>(new Set());

  useEffect(() => {
    nativeWa
      .chats(accountId)
      .then(setChats)
      .catch((e) => onError(errMsg(e)));
  }, [accountId, onError]);

  const term = q.trim().toLowerCase();
  const list = (chats ?? [])
    .filter((c) => c.id !== "status@broadcast" && !isChannel(c.id))
    .filter((c) => !term || (c.name ?? "").toLowerCase().includes(term) || c.id.includes(term))
    .slice(0, 50);

  const send = async (toChatId: string) => {
    setBusy(toChatId);
    try {
      await nativeWa.forward(accountId, fromChatId, message.id, toChatId);
      setDone((d) => new Set(d).add(toChatId));
    } catch (e) {
      onError(errMsg(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-4"
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div className="w-full max-w-[380px] max-h-full flex flex-col rounded-xl bg-white dark:bg-neutral-900 shadow-2xl">
        <div className="flex items-center gap-2 p-3 border-b border-neutral-200 dark:border-neutral-800">
          <span className="font-semibold flex-1">Forward to…</span>
          <button onClick={onClose}>
            <X size={16} />
          </button>
        </div>
        <div className="p-2 relative">
          <Search size={14} className="absolute left-4 top-4.5 text-neutral-400" />
          <Input className="pl-8" placeholder="Search" value={q} onChange={(e) => setQ(e.target.value)} autoFocus />
        </div>
        <div className="flex-1 overflow-y-auto">
          {list.map((c) => {
            const name = c.name || displayId(c.id);
            return (
              <div key={c.id} className="flex items-center gap-2 px-3 py-1.5">
                <Avatar name={name} size={30} />
                <span className="flex-1 truncate text-sm">{name}</span>
                <Button
                  size="sm"
                  variant={done.has(c.id) ? "secondary" : "primary"}
                  disabled={busy === c.id || done.has(c.id)}
                  onClick={() => void send(c.id)}
                >
                  {busy === c.id ? <Loader2 size={12} className="animate-spin" /> : done.has(c.id) ? "Sent" : "Send"}
                </Button>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
