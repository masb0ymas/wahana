/** The chat list of a native account: filter tabs, search, rows and their menus. */
import { Fragment, memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  Archive,
  CheckCheck,
  BellOff,
  EyeOff,
  ChevronLeft,
  ChevronRight,
  Search,
  Users,
  Megaphone,
  Pin,
  Tag,
  X,
  Trash2,
  CheckSquare,
  Plus,
  Check,
} from "lucide-react";
import { Avatar } from "@/components/ui";
import { confirm } from "@/components/Confirm";
import { cn, errMsg, formatTime, isChannel, isDirect, isGroup } from "@/lib/utils";
import { chatLabel } from "@/lib/chatLabel";
import { stripWaMarkdown } from "@/lib/waMarkdown";
import { nativeWa, type NativeAccount, type NativeChat, type NativeLabel } from "@/lib/nativeWa";
import { AckIcon } from "@/components/MessageExtras";
import { NativeFollowChannel } from "@/screens/whatsapp/NativeChannel";
import { usePicture } from "@/screens/whatsapp/usePicture";
import { useSettings } from "@/store/settings";
import { nativeChatKey } from "@/lib/account";
import { useDrafts } from "@/store/drafts";
import { NativeLabelsDialog, labelColorHex } from "@/screens/whatsapp/NativeLabelsDialog";
import { isMutedUntil, useChatPrefs } from "@/store/chatPrefs";
import { MuteControl } from "@/components/MuteControl";
import { useWhatsApp } from "@/store/whatsapp";
import { secs, WA_PIN_LIMIT, type Filter, sanitizeTabOrder } from "@/screens/whatsapp/shared";
import { NewChatDialog } from "@/screens/whatsapp/ChatDialogs";

export function ChatList({
  account,
  header,
  chats,
  selected,
  onSelect,
  onDeleted,
  error,
  width,
  fill,
}: {
  account: NativeAccount;
  header: React.ReactNode;
  chats: NativeChat[];
  selected: string | null;
  onSelect: (id: string) => void;
  onDeleted: (ids: string[]) => void;
  error: string | null;
  width?: number;
  /** Fill the parent width instead of a fixed pane width (embedded in a grid cell). */
  fill?: boolean;
}) {
  const [q, setQ] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [labels, setLabels] = useState<NativeLabel[]>([]);
  const [labelMap, setLabelMap] = useState<Record<string, string[]>>({});
  const [menu, setMenu] = useState<{ chat: NativeChat; x: number; y: number } | null>(null);
  const [labelsFor, setLabelsFor] = useState<NativeChat | null>(null);
  const [relabel, setRelabel] = useState(0);
  const [selecting, setSelecting] = useState(false);
  const [following, setFollowing] = useState(false);
  const [newChat, setNewChat] = useState(false);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const savedTabOrder = useSettings((s) => s.chatTabOrder);
  const [tabOrder, setTabOrder] = useState<Filter[]>(() => sanitizeTabOrder(savedTabOrder));
  const tabOrderRef = useRef(tabOrder);
  tabOrderRef.current = tabOrder;
  useEffect(() => {
    setTabOrder(sanitizeTabOrder(savedTabOrder));
  }, [savedTabOrder]);
  // Tabs are reordered by pointer (not HTML5 drag-and-drop, which Tauri's window drag-drop
  // handler breaks on Windows). Chips are found by ref to decide the drop slot.
  const chipRefs = useRef(new Map<Filter, HTMLButtonElement>());
  const tabStripRef = useRef<HTMLDivElement>(null);
  // Which way the filter strip can still scroll; drives the < > buttons beside it.
  const [tabScroll, setTabScroll] = useState({ overflow: false, left: false, right: false });
  const updateTabScroll = useCallback(() => {
    const el = tabStripRef.current;
    if (!el) return;
    const max = el.scrollWidth - el.clientWidth;
    const next = { overflow: max > 1, left: el.scrollLeft > 1, right: el.scrollLeft < max - 1 };
    setTabScroll((s) => (s.overflow === next.overflow && s.left === next.left && s.right === next.right ? s : next));
  }, []);
  useEffect(() => {
    const el = tabStripRef.current;
    if (!el) return;
    updateTabScroll();
    const ro = new ResizeObserver(updateTabScroll);
    ro.observe(el);
    return () => ro.disconnect();
  }, [updateTabScroll]);
  const scrollTabs = (dir: -1 | 1) => {
    const el = tabStripRef.current;
    if (el) el.scrollBy({ left: dir * el.clientWidth * 0.7, behavior: "smooth" });
  };
  const dragRef = useRef<{ id: Filter; x: number; y: number; active: boolean } | null>(null);
  const [draggingTab, setDraggingTab] = useState<Filter | null>(null);
  const suppressTabClick = useRef(false);
  const onTabPointerDown = (e: React.PointerEvent<HTMLButtonElement>, id: Filter) => {
    if (e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    dragRef.current = { id, x: e.clientX, y: e.clientY, active: false };
    suppressTabClick.current = false;
  };
  const onTabPointerMove = (e: React.PointerEvent<HTMLButtonElement>) => {
    const drag = dragRef.current;
    if (!drag) return;
    if (!drag.active) {
      // A small threshold keeps a plain click from starting a drag.
      if (Math.abs(e.clientX - drag.x) < 4 && Math.abs(e.clientY - drag.y) < 4) return;
      drag.active = true;
      setDraggingTab(drag.id);
    }
    // Dragging near either edge of the strip scrolls it, so hidden chips can be reached.
    const strip = tabStripRef.current;
    if (strip) {
      const s = strip.getBoundingClientRect();
      if (e.clientX < s.left + 24) strip.scrollLeft -= 8;
      else if (e.clientX > s.right - 24) strip.scrollLeft += 8;
    }
    // The nearest chip by its center decides the new slot, so wrapped rows work too.
    let nearest: Filter = drag.id;
    let best = Infinity;
    for (const [id, el] of chipRefs.current) {
      const r = el.getBoundingClientRect();
      const dist = Math.hypot(e.clientX - (r.left + r.width / 2), e.clientY - (r.top + r.height / 2));
      if (dist < best) {
        best = dist;
        nearest = id;
      }
    }
    if (nearest === drag.id) return;
    setTabOrder((order) => {
      const from = order.indexOf(drag.id);
      const to = order.indexOf(nearest);
      if (from < 0 || to < 0 || from === to) return order;
      const next = [...order];
      next.splice(from, 1);
      next.splice(to, 0, drag.id);
      return next;
    });
  };
  const onTabPointerUp = () => {
    const drag = dragRef.current;
    dragRef.current = null;
    if (!drag?.active) return;
    suppressTabClick.current = true;
    setDraggingTab(null);
    void useSettings.getState().save({ chatTabOrder: tabOrderRef.current });
  };
  const onTabPointerCancel = () => {
    dragRef.current = null;
    setDraggingTab(null);
  };
  // Keep the active filter's chip in view when it sits past the strip's scrolled edge.
  useEffect(() => {
    chipRefs.current.get(filter)?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [filter]);
  const pinned = useChatPrefs((s) => s.pinned);
  const muted = useChatPrefs((s) => s.muted);
  const blurred = useChatPrefs((s) => s.blurred);
  const archived = useChatPrefs((s) => s.archived);
  const labelsTick = useWhatsApp((s) => s.labelsTick[account.id] ?? 0);

  useEffect(() => {
    const focus = () => searchRef.current?.focus();
    window.addEventListener("wahana:focus-search", focus);
    return () => window.removeEventListener("wahana:focus-search", focus);
  }, []);

  useEffect(() => {
    let cancelled = false;
    Promise.all([nativeWa.labels(account.id), nativeWa.labelMap(account.id)])
      .then(([all, map]) => {
        if (cancelled) return;
        setLabels(all);
        setLabelMap(map);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [account.id, labelsTick, relabel]);

  const shown = useMemo(() => {
    const needle = q.trim().toLowerCase();
    const list = chats.filter((c) => {
      // Archived chats only show under the "archived" tab, and nowhere else.
      const isArchived = !!archived[nativeChatKey(account.id, c.id)];
      if (filter === "archived") {
        if (!isArchived) return false;
      } else if (isArchived) {
        return false;
      }
      if (filter === "unread" && c.unread === 0) return false;
      if (filter === "private" && !isDirect(c.id)) return false;
      if (filter === "groups" && !isGroup(c.id)) return false;
      if (filter === "community" && !c.community) return false;
      if (filter === "channels" && !isChannel(c.id)) return false;
      if (!needle) return true;
      return (
        c.name.toLowerCase().includes(needle) ||
        c.id.includes(needle) ||
        (c.phone ?? "").replace(/\D/g, "").includes(needle.replace(/\D/g, "") || "\0") ||
        c.lastText.toLowerCase().includes(needle)
      );
    });
    // The community filter groups by community name, keeping the recency order inside one.
    if (filter === "community") return [...list].sort((a, b) => (a.community ?? "").localeCompare(b.community ?? ""));
    // Pinned chats float to the top (most recently pinned first), like WhatsApp.
    return [...list].sort((a, b) => (pinned[nativeChatKey(account.id, b.id)] ?? 0) - (pinned[nativeChatKey(account.id, a.id)] ?? 0));
  }, [chats, q, filter, pinned, archived, account.id]);

  // Chats that vanish from the list (deleted elsewhere) drop out of the selection.
  useEffect(() => {
    setPicked((cur) => {
      const next = new Set([...cur].filter((id) => chats.some((c) => c.id === id)));
      return next.size === cur.size ? cur : next;
    });
  }, [chats]);
  useEffect(() => {
    setSelecting(false);
    setPicked(new Set());
  }, [account.id]);

  const exitSelect = () => {
    setSelecting(false);
    setPicked(new Set());
  };
  const toggle = (id: string) =>
    setPicked((cur) => {
      const next = new Set(cur);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  const totalUnread = chats.filter((c) => c.unread > 0).length;
  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    setActionError(null);
    try {
      await fn();
    } catch (e) {
      setActionError(errMsg(e));
    } finally {
      setBusy(false);
    }
  };
  const markRead = () =>
    run(async () => {
      await nativeWa.markAllRead(account.id, picked.size ? [...picked] : undefined);
      exitSelect();
    });
  const deletePicked = async () => {
    const ids = [...picked];
    if (ids.length === 0) return;
    const ok = await confirm({
      title: ids.length === 1 ? "Delete this chat?" : `Delete ${ids.length} chats?`,
      message: "Removes the conversations and their messages from this account and its linked phone. This cannot be undone.",
      confirmLabel: "Delete",
      danger: true,
    });
    if (!ok) return;
    await run(async () => {
      await nativeWa.deleteChats(account.id, ids);
      onDeleted(ids);
      exitSelect();
    });
  };
  const archivePicked = () =>
    run(async () => {
      const ids = [...picked];
      if (ids.length === 0) return;
      const on = filter !== "archived";
      const prefs = useChatPrefs.getState();
      for (const id of ids) prefs.toggle("archived", nativeChatKey(account.id, id), on);
      await Promise.all(ids.map((id) => nativeWa.archiveChat(account.id, id, on).catch(() => {})));
      exitSelect();
    });

  // Header pills. A grid tile drops the labels and grows the icons, so the tap target stays
  // comfortable in the narrow cell: 28x28 there, 28px tall with a label in the chat pane.
  const chipCls = "rounded-full px-2.5 py-1.5 text-[11px] font-medium capitalize whitespace-nowrap touch-none select-none";
  const actCls = cn(
    "inline-flex items-center justify-center gap-1 rounded-full font-medium bg-neutral-100 dark:bg-neutral-800 text-neutral-600 dark:text-neutral-300",
    fill ? "p-1.5" : "px-2.5 py-1.5 text-xs",
  );
  const actIcon = fill ? 16 : 14;

  return (
    <div
      style={fill ? undefined : { width }}
      className={cn(
        "flex flex-col bg-white dark:bg-neutral-900",
        fill ? "flex-1 min-h-0 w-full" : "shrink-0 border-r border-neutral-200 dark:border-neutral-800",
      )}
    >
      <div className="p-3 border-b border-neutral-200 dark:border-neutral-800 space-y-2">
        {header}
        {account.syncing != null && <div className="px-1 text-xs text-neutral-500">Syncing history… {account.syncing}%</div>}
        <div className="relative">
          <Search size={14} className="absolute left-2.5 top-2.5 text-neutral-400" />
          <input
            ref={searchRef}
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") setQ("");
              if (e.key === "Enter" && shown[0]) onSelect(shown[0].id);
            }}
            placeholder="Search chats (⌘K)"
            className="w-full rounded-lg bg-neutral-100 dark:bg-neutral-800 pl-8 pr-8 py-1.5 text-sm outline-none"
          />
          {q && (
            <button
              onClick={() => {
                setQ("");
                searchRef.current?.focus();
              }}
              title="Clear search"
              aria-label="Clear search"
              className="absolute right-2.5 top-2.5 text-neutral-400 hover:text-neutral-600 dark:hover:text-neutral-200"
            >
              <X size={14} />
            </button>
          )}
        </div>
        {/* Two rows: the filter chips scroll sideways when they don't fit (a vertical mouse wheel
            scrolls them too), and the actions sit on their own row below. */}
        <div className="flex flex-col gap-1.5">
          <div className="flex items-center gap-0.5">
            {tabScroll.overflow && (
              <button
                onClick={() => scrollTabs(-1)}
                disabled={!tabScroll.left}
                aria-label="Scroll filters left"
                className="shrink-0 rounded-full p-1 text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-800 disabled:opacity-30 disabled:hover:bg-transparent"
              >
                <ChevronLeft size={14} />
              </button>
            )}
            <div
              ref={tabStripRef}
              onScroll={updateTabScroll}
              onWheel={(e) => {
                if (Math.abs(e.deltaY) > Math.abs(e.deltaX)) e.currentTarget.scrollLeft += e.deltaY;
              }}
              className="no-scrollbar flex min-w-0 flex-1 items-center gap-1 overflow-x-auto"
            >
              {tabOrder.map((f) => (
                <button
                  key={f}
                  ref={(el) => {
                    if (el) chipRefs.current.set(f, el);
                    else chipRefs.current.delete(f);
                  }}
                  onPointerDown={(e) => onTabPointerDown(e, f)}
                  onPointerMove={onTabPointerMove}
                  onPointerUp={onTabPointerUp}
                  onPointerCancel={onTabPointerCancel}
                  onClick={() => {
                    if (suppressTabClick.current) {
                      suppressTabClick.current = false;
                      return;
                    }
                    setFilter(f);
                  }}
                  className={cn(
                    chipCls,
                    filter === f ? "bg-wa-dark text-white" : "bg-neutral-100 dark:bg-neutral-800 text-neutral-600 dark:text-neutral-300",
                    draggingTab === f && "cursor-grabbing opacity-70 scale-105",
                  )}
                >
                  {f}
                </button>
              ))}
            </div>
            {tabScroll.overflow && (
              <button
                onClick={() => scrollTabs(1)}
                disabled={!tabScroll.right}
                aria-label="Scroll filters right"
                className="shrink-0 rounded-full p-1 text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-800 disabled:opacity-30 disabled:hover:bg-transparent"
              >
                <ChevronRight size={14} />
              </button>
            )}
          </div>
          <div className={cn("flex items-center gap-1", !fill && "justify-end")}>
            {!selecting && filter === "channels" && (
              <button
                onClick={() => setFollowing(true)}
                title="Follow a channel from its link"
                aria-label="Follow a channel"
                className={actCls}
              >
                <Megaphone size={actIcon} /> {!fill && "Follow"}
              </button>
            )}
            {!selecting && totalUnread > 0 && (
              <button
                onClick={() => void markRead()}
                disabled={busy}
                title="Mark every chat as read"
                aria-label="Mark every chat as read"
                className={cn(actCls, "disabled:opacity-50")}
              >
                <CheckCheck size={actIcon} /> {!fill && "Read all"}
              </button>
            )}
            {!selecting && (
              <button onClick={() => setNewChat(true)} title="Start a chat with a phone number" aria-label="New chat" className={actCls}>
                <Plus size={actIcon} /> {!fill && "New chat"}
              </button>
            )}
            <button
              onClick={() => (selecting ? exitSelect() : setSelecting(true))}
              title={selecting ? "Cancel selection" : "Select chats"}
              aria-label={selecting ? "Cancel selection" : "Select chats"}
              className={cn(
                actCls,
                selecting ? "bg-wa-dark text-white" : "bg-neutral-100 dark:bg-neutral-800 text-neutral-600 dark:text-neutral-300",
              )}
            >
              {fill && selecting ? <X size={actIcon} /> : <CheckSquare size={actIcon} />} {!fill && (selecting ? "Cancel" : "Select")}
            </button>
          </div>
        </div>
        {selecting && (
          <div className="flex flex-wrap items-center gap-1.5 text-xs">
            <span className="text-neutral-500 mr-auto">{picked.size} selected</span>
            <button
              className="rounded-md px-2 py-1 hover:bg-neutral-100 dark:hover:bg-neutral-800"
              onClick={() => setPicked(picked.size === shown.length ? new Set() : new Set(shown.map((c) => c.id)))}
            >
              {picked.size === shown.length && shown.length > 0 ? "None" : "All"}
            </button>
            <button
              disabled={busy || picked.size === 0}
              onClick={() => void markRead()}
              className="inline-flex items-center gap-1 rounded-md px-2 py-1 hover:bg-neutral-100 dark:hover:bg-neutral-800 disabled:opacity-40"
            >
              <CheckCheck size={14} /> Read
            </button>
            <button
              disabled={busy || picked.size === 0}
              onClick={() => void archivePicked()}
              className="inline-flex items-center gap-1 rounded-md px-2 py-1 hover:bg-neutral-100 dark:hover:bg-neutral-800 disabled:opacity-40"
            >
              <Archive size={14} /> {filter === "archived" ? "Unarchive" : "Archive"}
            </button>
            <button
              disabled={busy || picked.size === 0}
              onClick={() => void deletePicked()}
              className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-red-600 hover:bg-red-50 dark:hover:bg-red-950/40 disabled:opacity-40"
            >
              <Trash2 size={14} /> Delete
            </button>
          </div>
        )}
      </div>
      {(error || account.error || actionError) && (
        <div className="px-3 py-2 text-xs bg-red-50 dark:bg-red-950/40 text-red-700 dark:text-red-300 selectable">
          {actionError ?? error ?? account.error}
        </div>
      )}
      <div className="flex-1 overflow-y-auto">
        {shown.length === 0 ? (
          <div className="p-4 text-xs text-neutral-500">
            {filter === "archived"
              ? "No archived chats."
              : chats.length > 0
                ? "No chats match."
                : account.status === "working"
                  ? "Your phone sends chat history only when a device is linked. If this account was linked before history support, log out and link it again to load your chats."
                  : "No chats yet."}
          </div>
        ) : (
          shown.map((c, i) => {
            const key = nativeChatKey(account.id, c.id);
            const chips = (labelMap[c.id] ?? [])
              .map((lid) => labels.find((l) => l.id === lid))
              .filter((l): l is NativeLabel => !!l)
              .map((l) => ({ name: l.name, color: labelColorHex(l.color) }));
            return (
              <Fragment key={c.id}>
                {filter === "community" && c.community !== shown[i - 1]?.community && (
                  <div className="px-3 py-1 text-[11px] uppercase text-neutral-500">{c.community}</div>
                )}
                <ChatRow
                  chat={c}
                  accountId={account.id}
                  connected={account.status === "working"}
                  active={!selecting && c.id === selected}
                  pinned={!!pinned[key]}
                  muted={isMutedUntil(muted[key])}
                  blurred={!!blurred[key]}
                  chips={chips}
                  selecting={selecting}
                  checked={picked.has(c.id)}
                  onClick={() => (selecting ? toggle(c.id) : onSelect(c.id))}
                  onMenu={(e) => {
                    e.preventDefault();
                    if (selecting) return toggle(c.id);
                    setMenu({ chat: c, x: e.clientX, y: e.clientY });
                  }}
                />
              </Fragment>
            );
          })
        )}
      </div>
      {following && <NativeFollowChannel accountId={account.id} onFollowed={(id) => onSelect(id)} onClose={() => setFollowing(false)} />}
      {newChat && <NewChatDialog chats={chats} onOpen={onSelect} onClose={() => setNewChat(false)} />}
      {menu && (
        <RowMenu
          accountId={account.id}
          chat={menu.chat}
          pinned={!!pinned[nativeChatKey(account.id, menu.chat.id)]}
          waPinsFull={chats.filter((c) => (c.pinnedAt ?? 0) > 0).length >= WA_PIN_LIMIT}
          muted={muted[nativeChatKey(account.id, menu.chat.id)]}
          blurred={!!blurred[nativeChatKey(account.id, menu.chat.id)]}
          archived={!!archived[nativeChatKey(account.id, menu.chat.id)]}
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          onLabels={() => {
            setLabelsFor(menu.chat);
            setMenu(null);
          }}
        />
      )}
      {labelsFor && (
        <NativeLabelsDialog
          accountId={account.id}
          chatId={labelsFor.id}
          chatName={labelsFor.name}
          onChanged={() => setRelabel((t) => t + 1)}
          onClose={() => setLabelsFor(null)}
        />
      )}
    </div>
  );
}

/** Right-click menu on a native chat: pin, archive, mute, blur, labels. */
export function RowMenu({
  accountId,
  chat,
  pinned,
  waPinsFull,
  muted,
  blurred,
  archived,
  x,
  y,
  onClose,
  onLabels,
}: {
  accountId: string;
  chat: NativeChat;
  pinned: boolean;
  /** WhatsApp already has its three pins, so a new one is kept only in this app. */
  waPinsFull: boolean;
  muted: number | undefined;
  blurred: boolean;
  archived: boolean;
  x: number;
  y: number;
  onClose: () => void;
  onLabels: () => void;
}) {
  const togglePref = useChatPrefs((s) => s.toggle);
  const setPinned = useChatPrefs((s) => s.setPinned);
  const setMuted = useChatPrefs((s) => s.setMuted);
  const key = nativeChatKey(accountId, chat.id);
  const onWa = (chat.pinnedAt ?? 0) > 0;
  const localOnly = !pinned && waPinsFull;
  const item = "w-full flex items-center gap-2 px-3 py-1.5 text-left hover:bg-neutral-100 dark:hover:bg-neutral-800";
  // On `document.body`, so the menu sits at the pointer's viewport coordinates and its backdrop
  // covers the window even when the chat list is a grid tile (whose layout containment would
  // otherwise make `fixed` resolve against the tile).
  return createPortal(
    <div
      className="fixed inset-0 z-40"
      onClick={onClose}
      onContextMenu={(e) => {
        e.preventDefault();
        onClose();
      }}
    >
      <div
        className="absolute w-48 rounded-lg border border-neutral-200 dark:border-neutral-700 bg-white dark:bg-neutral-900 shadow-xl py-1 text-xs"
        style={{ left: Math.min(x, window.innerWidth - 200), top: Math.min(y, window.innerHeight - 220) }}
        onMouseDown={(e) => e.stopPropagation()}
        onClick={(e) => e.stopPropagation()}
      >
        <button
          className={item}
          title={localOnly ? `WhatsApp allows ${WA_PIN_LIMIT} pins; this one stays in Wahana only` : undefined}
          onClick={() => {
            setPinned(key, pinned ? null : Date.now());
            // Unpinning undoes the WhatsApp pin only where there is one; pinning goes to
            // WhatsApp while it has room, and is kept here alone if WhatsApp turns it down.
            const sync = pinned ? onWa : !localOnly;
            void nativeWa
              .pinChat(accountId, chat.id, !pinned, sync)
              .catch(() => (sync && !pinned ? nativeWa.pinChat(accountId, chat.id, true, false) : undefined))
              .catch(() => {});
            onClose();
          }}
        >
          <Pin size={13} /> {pinned ? "Unpin" : localOnly ? "Pin in Wahana only" : "Pin to top"}
        </button>
        <button
          className={item}
          title="Hide this chat from the main list; it stays in the Archived tab"
          onClick={() => {
            togglePref("archived", key);
            void nativeWa.archiveChat(accountId, chat.id, !archived).catch(() => {});
            onClose();
          }}
        >
          <Archive size={13} /> {archived ? "Unarchive" : "Archive"}
        </button>
        <MuteControl
          className={item}
          until={muted}
          onSet={(until) => {
            setMuted(key, until);
            void nativeWa.muteChat(accountId, chat.id, until).catch(() => {});
            onClose();
          }}
        />
        <button
          className={item}
          title="Blur the preview in the chat list and message bubbles until hovered"
          onClick={() => {
            togglePref("blurred", key);
            onClose();
          }}
        >
          <EyeOff size={13} /> {blurred ? "Remove blur" : "Blur preview"}
        </button>
        <button className={item} onClick={onLabels}>
          <Tag size={13} /> Labels…
        </button>
      </div>
    </div>,
    document.body,
  );
}

export const ChatRow = memo(function ChatRow({
  chat,
  accountId,
  connected,
  active,
  pinned,
  muted,
  blurred,
  chips,
  selecting,
  checked,
  onClick,
  onMenu,
}: {
  chat: NativeChat;
  accountId: string;
  connected: boolean;
  active: boolean;
  pinned: boolean;
  muted: boolean;
  blurred: boolean;
  chips: { name: string; color: string }[];
  selecting: boolean;
  checked: boolean;
  onClick: () => void;
  onMenu: (e: React.MouseEvent) => void;
}) {
  const picture = usePicture(accountId, chat.id, connected);
  const group = isGroup(chat.id);
  const channel = isChannel(chat.id);
  const { title, pushName } = chatLabel(chat, chat.id);
  const body = stripWaMarkdown(chat.lastText);
  const sender = group ? (chat.lastFromMe ? "You" : chat.lastSender.split(/\s+/)[0]) : "";
  const preview = sender ? `${sender}: ${body}` : body;
  // translateZ(0) forces GPU compositing so WKWebView uses grayscale antialiasing; without it the
  // blurred text rasterizes with subpixel AA and shows a coloured (purple) fringe.
  const blurCls = blurred && "blur-[3px] transform-gpu group-hover:blur-none transition-[filter] duration-150";
  // An unsent draft replaces the preview, like WhatsApp, except in the chat being typed in.
  const draft = useDrafts((s) => (active ? "" : (s.drafts[nativeChatKey(accountId, chat.id)] ?? "")));
  return (
    <button
      onClick={onClick}
      onContextMenu={onMenu}
      className={cn(
        "group w-full flex items-center gap-3 px-3 py-2.5 text-left hover:bg-neutral-100 dark:hover:bg-neutral-800",
        (active || checked) && "bg-neutral-100 dark:bg-neutral-800",
      )}
    >
      {selecting && (
        <span
          className={cn(
            "shrink-0 w-4 h-4 rounded border grid place-items-center",
            checked ? "bg-wa border-wa text-white" : "border-neutral-400",
          )}
        >
          {checked && <Check size={12} />}
        </span>
      )}
      <Avatar src={picture} name={pushName ?? title} size={44} />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1">
          {group ? (
            <Users size={12} className="text-neutral-400 shrink-0" />
          ) : channel ? (
            <Megaphone size={12} className="text-neutral-400 shrink-0" />
          ) : null}
          <span className={cn("font-medium truncate shrink-0 max-w-[70%]", blurCls)}>{title}</span>
          {pushName && <span className={cn("text-[11px] text-neutral-400 truncate", blurCls)}>~{pushName}</span>}
          {muted && <BellOff size={12} className="shrink-0 text-neutral-400" />}
          {blurred && <EyeOff size={12} className="shrink-0 text-neutral-400" />}
          {pinned && <Pin size={12} className="shrink-0 text-neutral-400" />}
          {chat.lastTimestamp > 0 && (
            <span className="ml-auto shrink-0 text-[11px] text-neutral-400">{formatTime(secs(chat.lastTimestamp))}</span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <div
            className={cn(
              "text-xs truncate flex-1",
              chat.unread ? "text-neutral-800 dark:text-neutral-100 font-medium" : "text-neutral-500",
              blurCls,
            )}
          >
            {draft ? (
              <>
                <span className="text-red-500 font-medium">Draft:</span>
                {draft}
              </>
            ) : (
              <>
                {chat.lastFromMe && <AckIcon ack={chat.lastAck} className="inline mr-1 -mt-0.5" />}
                {preview}
              </>
            )}
          </div>
          {chat.unread > 0 && (
            <span className="shrink-0 min-w-[18px] h-[18px] px-1 rounded-full bg-wa text-[10px] font-bold text-white grid place-items-center">
              {chat.unread}
            </span>
          )}
        </div>
        {chips.length > 0 && (
          <div className="flex flex-wrap gap-1 mt-0.5">
            {chips.slice(0, 3).map((c, i) => (
              <span
                key={i}
                className="inline-flex items-center gap-1 rounded-full bg-neutral-100 dark:bg-neutral-800 px-1.5 py-0.5 text-[10px] text-neutral-600 dark:text-neutral-300"
              >
                <span className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: c.color }} />
                <span className="truncate max-w-[80px]">{c.name}</span>
              </span>
            ))}
          </div>
        )}
      </div>
    </button>
  );
});

// ── Conversation ─────────────────────────────────────────────────────────
