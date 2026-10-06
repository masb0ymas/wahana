/** One open chat: its header, message list and composer. */
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { CheckCheck, ChevronLeft, Languages, Loader2, Sparkles, Megaphone, Info } from "lucide-react";
import { Avatar, Button, Popover } from "@/components/ui";
import { TakeoverButton } from "@/components/TakeoverButton";
import { confirm } from "@/components/Confirm";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { LANGUAGES, aiConfigured, langName } from "@/lib/ai";
import { cn, convKey, errMsg, formatDateDivider, formatLastSeen, isChannel, isGroup } from "@/lib/utils";
import { chatLabel } from "@/lib/chatLabel";
import { stripWaMarkdown } from "@/lib/waMarkdown";
import { nativeWa, type NativeAccount, type NativeChat, type NativeMessage } from "@/lib/nativeWa";
import { NativeMessageMenu, NativeSummaryModal, suggestRepliesFor, useNativeAutoTranslate } from "@/screens/whatsapp/NativeAi";
import { NativeMessageInfo } from "@/screens/whatsapp/NativeMessageInfo";
import { NativeInfoPanel } from "@/screens/whatsapp/NativeInfoPanel";
import { usePicture } from "@/screens/whatsapp/usePicture";
import { useNativeTyping } from "@/screens/whatsapp/useNativeTyping";
import { TypingBubble } from "@/components/TypingBubble";
import { saveNativeMedia } from "@/screens/whatsapp/NativeMediaView";
import { readReceiptsFor, useAppearOnline, useReadReceipts } from "@/store/settings";
import { nativeAccountKey, nativeChatKey } from "@/lib/account";
import { useChatPrefs } from "@/store/chatPrefs";
import { bareId } from "@/store/reactions";
import { isPinned, useChatPins, usePins } from "@/store/pins";
import { PinBanner } from "@/components/PinBanner";
import { secs, EDIT_WINDOW_MS, PAGE, type CrossReply, type Attachment } from "@/screens/whatsapp/shared";
import { Bubble } from "@/screens/whatsapp/Bubble";
import { NativeForwardDialog } from "@/screens/whatsapp/ChatDialogs";
import { Composer } from "@/screens/whatsapp/Composer";

export function Conversation({
  account,
  chatId,
  chat,
  tick,
  onOpenChat,
  initialReply,
  onReplyUsed,
  initialJump,
  onJumpUsed,
  onError,
  onBack,
  compact,
  infoOverlay,
}: {
  account: NativeAccount;
  chatId: string;
  chat: NativeChat | undefined;
  tick: number;
  /** Open a direct chat: candidate chat ids, best first, optionally quoting a message and/or scrolling to one. */
  onOpenChat: (ids: string[], reply?: CrossReply, jump?: string) => void;
  /** A cross-chat reply (reply privately) waiting for this chat's composer, taken once. */
  initialReply: CrossReply | null;
  onReplyUsed: () => void;
  /** A message id to scroll to once this chat is mounted (opening a cross-chat quote), taken once. */
  initialJump: string | null;
  onJumpUsed: () => void;
  onError: (e: string) => void;
  /** Shown as a back button (top-left) when the conversation is embedded in a tile. */
  onBack?: () => void;
  /** Choose the compact composer layout (tools above the input). */
  compact?: boolean;
  /** Open the info panel over the conversation instead of beside it (embedded in a tile). */
  infoOverlay?: boolean;
}) {
  const [info, setInfo] = useState(false);
  /** A group member whose profile was opened from a bubble; replaces the chat's own info panel. */
  const [profileId, setProfileId] = useState<string | null>(null);
  // The file waiting to be sent, owned here so dropping it anywhere in the chat (not only on the
  // composer) attaches it. Reset per chat because the screen keys this component by chat.
  const [attachment, setAttachment] = useState<Attachment | null>(null);
  const [dragging, setDragging] = useState(false);
  /** Nested enter/leave count so moving over child elements does not flicker the drop highlight. */
  const dragDepth = useRef(0);
  const attach = (file: File) => setAttachment({ file, preview: file.type.startsWith("image/") ? URL.createObjectURL(file) : null });
  useEffect(() => () => void (attachment?.preview && URL.revokeObjectURL(attachment.preview)), [attachment]);
  const readMode = useReadReceipts(nativeAccountKey(account.id));
  const { title, pushName } = chatLabel(chat, chatId);
  const name = pushName ?? title;
  const prefsKey = convKey(account.id, chatId);
  const autoTranslate = useChatPrefs((s) => s.autoTranslate[prefsKey]);
  const blurred = useChatPrefs((s) => !!s.blurred[nativeChatKey(account.id, chatId)]);

  // Whatever arrives in the chat on screen is read as it lands. Lives here (not in the
  // screen) so an open conversation is marked read wherever it is embedded, e.g. a grid tile.
  const openUnread = chat?.unread ?? 0;
  useEffect(() => {
    if (openUnread === 0) return;
    void nativeWa.markRead(account.id, chatId).catch((e) => onError(errMsg(e)));
    // Blue ticks to the sender only when the account is set to receipt on open.
    if (readReceiptsFor(nativeAccountKey(account.id)) === "always") void nativeWa.sendReceipt(account.id, chatId).catch(() => {});
  }, [account.id, chatId, openUnread, onError]);
  const [infoFor, setInfoFor] = useState<NativeMessage | null>(null);
  const [menu, setMenu] = useState<{ m: NativeMessage; pos: { x: number; y: number } } | null>(null);
  const [summary, setSummary] = useState(false);
  const [draftPick, setDraftPick] = useState<string | null>(null);
  const [replyTo, setReplyTo] = useState<NativeMessage | null>(null);
  /** Source chat of a cross-chat reply (reply privately); null when replying within this chat. */
  const [replyContext, setReplyContext] = useState<{ chatId: string; chatName: string } | null>(null);
  const [editing, setEditing] = useState<NativeMessage | null>(null);
  const [forward, setForward] = useState<NativeMessage | null>(null);
  const pins = usePins((s) => s.items);
  const [messages, setMessages] = useState<NativeMessage[]>([]);
  const [limit, setLimit] = useState(PAGE);
  /** Waiting for the phone to answer a request for older messages. */
  const [asking, setAsking] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);
  const atBottom = useRef(true);
  /** Scroll position to restore after older messages are prepended. */
  const anchor = useRef<{ height: number; top: number } | null>(null);
  const group = isGroup(chatId);
  const channel = isChannel(chatId);
  const connected = account.status === "working";
  /** Owners and admins of a channel can post, edit and delete; everyone else only reads. */
  const [canPost, setCanPost] = useState(false);
  useEffect(() => {
    setCanPost(false);
    if (!channel || !connected) return;
    let cancelled = false;
    nativeWa
      .chatInfo(account.id, chatId)
      .then((d) => !cancelled && setCanPost(d.type === "channel" && (d.role === "owner" || d.role === "admin")))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [channel, connected, account.id, chatId]);
  /** Group admins can delete other people's messages for everyone. */
  const [amAdmin, setAmAdmin] = useState(false);
  useEffect(() => {
    setAmAdmin(false);
    if (!group || !connected) return;
    let cancelled = false;
    nativeWa
      .chatInfo(account.id, chatId)
      .then((d) => !cancelled && setAmAdmin(d.type === "group" && d.members.some((m) => m.isMe && m.admin)))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [group, connected, account.id, chatId]);
  const picture = usePicture(account.id, chatId, connected);
  const appearOnline = useAppearOnline(nativeAccountKey(account.id));
  const watched = useNativeTyping(account.id, chatId, connected && !channel && appearOnline);
  const typists = Object.values(watched.typists);
  const presence = group ? null : watched.presence;
  const moreStored = messages.length >= limit;
  const chatPins = useChatPins(prefsKey);
  const [pinIdx, setPinIdx] = useState(0);
  /** Pinned message to scroll to once it is loaded. */
  const [jumpTo, setJumpTo] = useState<string | null>(null);
  const [flash, setFlash] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    nativeWa
      .messages(account.id, chatId, limit)
      .then((list) => {
        if (cancelled) return;
        setMessages((prev) => {
          if (list.length > prev.length) setAsking(false);
          return list;
        });
      })
      .catch((e) => onError(errMsg(e)));
    return () => {
      cancelled = true;
    };
  }, [account.id, chatId, limit, tick, onError]);

  // The phone may not answer at all (e.g. it is offline); don't spin forever.
  useEffect(() => {
    if (!asking) return;
    const timer = setTimeout(() => setAsking(false), 20_000);
    return () => clearTimeout(timer);
  }, [asking]);

  // Keep the view where it was when older messages land on top; otherwise follow new
  // messages while scrolled to the bottom.
  useLayoutEffect(() => {
    const el = listRef.current;
    if (!el) return;
    if (anchor.current) {
      el.scrollTop = el.scrollHeight - anchor.current.height + anchor.current.top;
      anchor.current = null;
    } else if (atBottom.current) {
      el.scrollTop = el.scrollHeight;
    }
  }, [messages, typists.length]);

  // Scroll to a pinned message, paging in stored history until it shows up.
  useEffect(() => {
    if (!jumpTo) return;
    const el = listRef.current?.querySelector<HTMLElement>(`[data-msg="${CSS.escape(jumpTo)}"]`);
    if (el) {
      el.scrollIntoView({ block: "center", behavior: "smooth" });
      setFlash(jumpTo);
      setJumpTo(null);
    } else if (messages.length === 0) {
      // The first page is still loading; decide once it arrives.
    } else if (moreStored) {
      atBottom.current = false;
      setLimit((l) => l + PAGE);
    } else {
      setJumpTo(null);
      onError("That message isn't stored yet. Load older messages from your phone first.");
    }
  }, [jumpTo, messages, moreStored, onError]);

  useEffect(() => {
    if (!flash) return;
    const timer = setTimeout(() => setFlash(null), 1500);
    return () => clearTimeout(timer);
  }, [flash]);

  useNativeAutoTranslate(messages, autoTranslate?.in);
  const onMenu = useCallback((m: NativeMessage, pos: { x: number; y: number }) => setMenu({ m, pos }), []);

  // A reply, an edit, or a forward does not survive a chat switch.
  useEffect(() => {
    setReplyTo(null);
    setReplyContext(null);
    setEditing(null);
    setForward(null);
    setPinIdx(0);
    setJumpTo(null);
  }, [chatId]);

  // "Reply privately" opens a new chat quoting the source message. Declared after the reset above
  // so it wins on the mount that opens the chat.
  useEffect(() => {
    if (initialReply == null) return;
    setEditing(null);
    setReplyTo(initialReply.message);
    setReplyContext({ chatId: initialReply.chatId, chatName: initialReply.chatName });
    onReplyUsed();
  }, [initialReply]); // eslint-disable-line react-hooks/exhaustive-deps

  // Opening a cross-chat quote switches to its chat and scrolls to the message there.
  useEffect(() => {
    if (initialJump == null) return;
    setJumpTo(initialJump);
    onJumpUsed();
  }, [initialJump]); // eslint-disable-line react-hooks/exhaustive-deps

  const deleteMessage = async (m: NativeMessage) => {
    const choice = await confirm({
      title: "Delete message?",
      choices: [
        {
          id: "everyone",
          label: "Delete for everyone",
          hint: m.fromMe
            ? "Removes it from the chat for all participants."
            : "Removes it from the chat for all participants, as a group admin.",
          danger: true,
        },
      ],
    });
    if (choice !== "everyone") return;
    try {
      await nativeWa.deleteMessage(account.id, chatId, m.id);
    } catch (e) {
      onError(errMsg(e));
    }
  };

  const deleteForMe = async (m: NativeMessage) => {
    try {
      await nativeWa.deleteLocal(account.id, chatId, m.id);
      setMessages((list) => list.filter((x) => x.id !== m.id));
    } catch (e) {
      onError(errMsg(e));
    }
  };

  // The sender's direct chat may run on their privacy id (`@lid`) rather than their phone number,
  // so offer both and let the caller open whichever chat already exists (mirrors `chatIdsFor`).
  const senderChatIds = (m: NativeMessage) => {
    const ids = [m.senderId, m.senderPhone ? `${m.senderPhone.replace(/\D/g, "")}@s.whatsapp.net` : null].filter(
      (id): id is string => !!id,
    );
    return ids.length ? ids : null;
  };

  /** Pin for `secs`, or unpin when `secs` is omitted. */
  const pinMessage = async (m: NativeMessage, secs?: number) => {
    const on = secs !== undefined;
    try {
      await nativeWa.pinMessage(account.id, chatId, m.id, on, secs);
      usePins.getState().set(prefsKey, m.id, on ? Date.now() + secs * 1000 : 0);
    } catch (e) {
      onError(errMsg(e));
    }
  };

  // A channel's history lives on the server: fetch the latest page (with reaction totals)
  // on open and every minute while it stays open.
  useEffect(() => {
    if (!channel || !connected) return;
    const sync = () => nativeWa.channelSync(account.id, chatId, false).catch(() => {});
    void sync();
    const timer = setInterval(sync, 60_000);
    return () => clearInterval(timer);
  }, [channel, connected, account.id, chatId]);

  const loadOlder = () => {
    const el = listRef.current;
    anchor.current = el ? { height: el.scrollHeight, top: el.scrollTop } : null;
    setLimit((l) => l + PAGE);
    if (moreStored) return;
    setAsking(true);
    if (channel) {
      nativeWa
        .channelSync(account.id, chatId, true)
        .then((n) => {
          // Nothing newly stored means the start of the channel: stop spinning.
          if (n === 0) setAsking(false);
        })
        .catch((e) => {
          setAsking(false);
          onError(errMsg(e));
        });
      return;
    }
    nativeWa.loadOlder(account.id, chatId).catch((e) => {
      setAsking(false);
      onError(errMsg(e));
    });
  };

  // Sending our own message pulls the view to the newest even if the user had scrolled up.
  const scrollToLatest = useCallback(() => {
    atBottom.current = true;
    anchor.current = null;
    const el = listRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, []);

  return (
    <>
      <div
        // Dropping a file anywhere in the chat attaches it, not only onto the composer.
        onDragEnter={(e) => {
          if (!e.dataTransfer.types.includes("Files")) return;
          e.preventDefault();
          dragDepth.current++;
          setDragging(true);
        }}
        onDragOver={(e) => {
          if (!e.dataTransfer.types.includes("Files")) return;
          e.preventDefault();
        }}
        onDragLeave={(e) => {
          if (!e.dataTransfer.types.includes("Files")) return;
          dragDepth.current = Math.max(0, dragDepth.current - 1);
          if (dragDepth.current === 0) setDragging(false);
        }}
        onDrop={(e) => {
          e.preventDefault();
          dragDepth.current = 0;
          setDragging(false);
          const f = e.dataTransfer.files[0];
          if (f) attach(f);
        }}
        className="relative flex-1 min-w-0 flex flex-col chat-wallpaper"
      >
        {/* Drawn above the header and composer, which would otherwise hide an inset ring. */}
        {dragging && <div className="pointer-events-none absolute inset-0 z-20 ring-2 ring-inset ring-wa-dark" />}
        <header
          className={cn(
            "shrink-0 flex items-center bg-white dark:bg-neutral-900 border-b border-neutral-200 dark:border-neutral-800",
            compact ? "h-12 gap-2 px-2.5" : "h-14 gap-3 px-4",
          )}
        >
          {onBack && (
            <button
              onClick={onBack}
              title="Back to chats"
              className="shrink-0 -ml-1 p-1 rounded text-neutral-500 hover:text-neutral-800 dark:hover:text-neutral-200"
            >
              <ChevronLeft size={20} />
            </button>
          )}
          <button
            className={cn("group flex items-center min-w-0 flex-1 text-left", compact ? "gap-2" : "gap-3")}
            onClick={() => {
              setProfileId(null);
              setInfo((v) => !v);
            }}
            title={group ? "Group info" : channel ? "Channel info" : "Contact info"}
          >
            <Avatar src={picture} name={name} size={compact ? 32 : 36} />
            <div className={cn("min-w-0", blurred && "blur-[3px] transform-gpu group-hover:blur-none transition-[filter] duration-150")}>
              <div className="font-medium truncate">{title}</div>
              <div className="text-xs truncate text-neutral-500">
                {group
                  ? "Group · click for members"
                  : channel
                    ? "Channel · read-only"
                    : presence?.online
                      ? "online"
                      : presence?.lastSeen
                        ? `last seen ${formatLastSeen(presence.lastSeen)}`
                        : pushName
                          ? `~${pushName}`
                          : chat?.saved && chat.phone
                            ? chat.phone
                            : "Contact"}
              </div>
            </div>
          </button>
          {readMode === "manual" && !channel && <NativeReadReceiptButton accountId={account.id} chatId={chatId} messages={messages} />}
          {!channel && <TakeoverButton account={nativeAccountKey(account.id)} chatId={chatId} name={title} />}
          <AutoTranslateButton prefsKey={prefsKey} />
          {aiConfigured() && (
            <Button variant="ghost" size="sm" onClick={() => setSummary(true)} title="Summarize with AI">
              <Sparkles size={16} />
            </Button>
          )}
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setInfo((v) => !v)}
            title="Info"
            className={cn(info && "text-wa-dark dark:text-wa")}
          >
            <Info size={16} />
          </Button>
        </header>

        {chatPins.length > 0 &&
          (() => {
            const index = pinIdx % chatPins.length;
            const id = chatPins[index]!;
            const m = messages.find((x) => bareId(x.id) === id);
            return (
              <PinBanner
                key={id}
                count={chatPins.length}
                index={index}
                who={m && (m.fromMe ? "You" : m.senderName || m.senderPhone || undefined)}
                text={m && (stripWaMarkdown(m.body) || "📎 Media")}
                onJump={() => {
                  setJumpTo(id);
                  // Like WhatsApp: each click moves on to the next (older) pin.
                  setPinIdx((i) => (i + 1) % chatPins.length);
                }}
                onUnpin={async () => {
                  await nativeWa.pinMessage(account.id, chatId, m?.id ?? id, false);
                  usePins.getState().set(prefsKey, id, 0);
                }}
              />
            );
          })()}

        <div
          ref={listRef}
          className={cn("@container flex-1 overflow-y-auto", compact ? "px-2.5 py-2" : "px-6 py-4")}
          onScroll={(e) => {
            const el = e.currentTarget;
            atBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
            // Stored pages load as you scroll up; asking the phone stays an explicit click.
            if (el.scrollTop < 40 && moreStored && !anchor.current) loadOlder();
          }}
        >
          <div className="flex justify-center my-2 min-h-6">
            {moreStored ? (
              <Loader2 size={16} className="animate-spin text-neutral-400" />
            ) : asking ? (
              <span className="flex items-center gap-1.5 text-[11px] text-neutral-500">
                <Loader2 size={12} className="animate-spin" /> {channel ? "Loading older posts…" : "Asking your phone for older messages…"}
              </span>
            ) : connected && messages.length > 0 ? (
              <button
                onClick={loadOlder}
                className="rounded-md bg-white/80 dark:bg-neutral-800 px-2 py-0.5 text-[11px] text-wa-dark dark:text-wa shadow-sm hover:bg-white dark:hover:bg-neutral-700"
              >
                {channel ? "Load older posts" : "Load older messages from your phone"}
              </button>
            ) : messages.length === 0 ? (
              <span className="rounded-md bg-white/80 dark:bg-neutral-800 px-2 py-0.5 text-[11px] text-neutral-600 dark:text-neutral-300 shadow-sm">
                No messages stored for this chat yet
              </span>
            ) : null}
          </div>
          {messages.map((m, i) => {
            const prev = messages[i - 1];
            // Photos sent together are one bubble, drawn by the first of them.
            const inAlbum = (a: NativeMessage | undefined, b: NativeMessage | undefined) =>
              !!a && !!b && !!a.albumId && a.albumId === b.albumId && !!a.media && a.fromMe === b.fromMe && a.senderName === b.senderName;
            if (inAlbum(prev, m)) return <div key={m.id} data-msg={bareId(m.id)} />;
            let album: NativeMessage[] | undefined;
            if (m.albumId && m.media) {
              album = [m];
              while (inAlbum(m, messages[i + album.length])) album.push(messages[i + album.length]!);
              if (album.length < 2) album = undefined;
            }
            const newDay = !prev || new Date(prev.timestamp).toDateString() !== new Date(m.timestamp).toDateString();
            const showSender = group && !m.fromMe && (newDay || prev?.fromMe || prev?.senderName !== m.senderName);
            const next = messages[i + (album?.length ?? 1)];
            // The last bubble of a run from one speaker (the run also ends at a day divider) carries the
            // tail and the group avatar, and gets a little more air below it so each run reads as one turn.
            const turnEnds =
              !next ||
              next.fromMe !== m.fromMe ||
              next.senderName !== m.senderName ||
              new Date(next.timestamp).toDateString() !== new Date(m.timestamp).toDateString();
            const showAvatar = !m.fromMe && group && turnEnds;
            return (
              <div
                key={m.id}
                data-msg={bareId(m.id)}
                className={cn(turnEnds ? "pb-2.5" : "pb-1", "rounded transition-colors duration-500", flash === bareId(m.id) && "bg-wa/25")}
              >
                {newDay && (
                  <div className="flex justify-center my-3">
                    <span className="rounded-md bg-white/80 dark:bg-neutral-800 px-2 py-0.5 text-[11px] text-neutral-600 dark:text-neutral-300 shadow-sm">
                      {formatDateDivider(secs(m.timestamp))}
                    </span>
                  </div>
                )}
                <ErrorBoundary inline label="message">
                  <Bubble
                    accountId={account.id}
                    connected={connected}
                    message={m}
                    album={album}
                    showSender={showSender}
                    avatar={!group ? "none" : showAvatar ? "show" : "space"}
                    avatarChatId={m.senderPhone ? `${m.senderPhone.replace(/\D/g, "")}@s.whatsapp.net` : null}
                    pinned={isPinned(pins, prefsKey, m.id)}
                    blurred={blurred}
                    tail={turnEnds}
                    onMenu={onMenu}
                    onPickReply={setDraftPick}
                    onJumpTo={(reply) => {
                      // A cross-chat quote (reply privately) lives in another chat: switch to it and scroll there.
                      if (reply.chat) onOpenChat([reply.chat], undefined, bareId(reply.id));
                      else setJumpTo(bareId(reply.id));
                    }}
                    onProfile={setProfileId}
                    onOpenNumber={(phone) => onOpenChat([`${phone.replace(/\D/g, "")}@s.whatsapp.net`])}
                  />
                </ErrorBoundary>
              </div>
            );
          })}
          {typists.map((t, i) => (
            <TypingBubble key={i} who={group ? (t.name ?? undefined) : undefined} recording={t.recording} />
          ))}
        </div>

        {channel && !canPost ? (
          <div className="shrink-0 flex items-center justify-center gap-2 bg-white dark:bg-neutral-900 border-t border-neutral-200 dark:border-neutral-800 px-4 py-3 text-xs text-neutral-500">
            <Megaphone size={14} /> Channels are read-only — only the channel can post.
          </div>
        ) : (
          <Composer
            account={account}
            chatId={chatId}
            chatName={name}
            messages={messages}
            autoOut={autoTranslate?.out}
            picked={draftPick}
            onPicked={() => setDraftPick(null)}
            onPick={setDraftPick}
            onError={onError}
            replyTo={replyTo}
            replyContext={replyContext}
            editing={editing}
            onCancelReply={() => {
              setReplyTo(null);
              setReplyContext(null);
            }}
            onCancelEdit={() => setEditing(null)}
            onSent={scrollToLatest}
            attachment={attachment}
            setAttachment={setAttachment}
            attach={attach}
            compact={compact}
          />
        )}
        {menu && (
          <NativeMessageMenu
            accountId={account.id}
            message={menu.m}
            pos={menu.pos}
            pinned={isPinned(pins, prefsKey, menu.m.id)}
            onSave={() => saveNativeMedia(account.id, menu.m).catch((e) => onError(errMsg(e)))}
            onReply={
              channel || menu.m.revokedAt
                ? undefined
                : () => {
                    setEditing(null);
                    setReplyContext(null);
                    setReplyTo(menu.m);
                  }
            }
            onEdit={
              // Only plain text can be edited (a caption edit needs the media message), and
              // only within WhatsApp's edit window.
              (!channel || canPost) &&
              !menu.m.revokedAt &&
              menu.m.fromMe &&
              menu.m.kind === "text" &&
              Date.now() - menu.m.timestamp < EDIT_WINDOW_MS
                ? () => {
                    setReplyTo(null);
                    setReplyContext(null);
                    setEditing(menu.m);
                  }
                : undefined
            }
            onDelete={
              (!channel || canPost) && !menu.m.revokedAt && (menu.m.fromMe || (group && amAdmin))
                ? () => void deleteMessage(menu.m)
                : undefined
            }
            onDeleteLocal={channel ? undefined : () => void deleteForMe(menu.m)}
            onChat={group && !menu.m.fromMe && senderChatIds(menu.m) ? () => onOpenChat(senderChatIds(menu.m)!) : undefined}
            onReplyPrivately={
              group && !menu.m.fromMe && !menu.m.revokedAt && senderChatIds(menu.m)
                ? () => onOpenChat(senderChatIds(menu.m)!, { message: menu.m, chatId, chatName: name })
                : undefined
            }
            onSuggestReply={
              channel && !canPost
                ? undefined
                : () =>
                    suggestRepliesFor({
                      id: menu.m.id,
                      accountId: account.id,
                      chatId,
                      chatName: name,
                      messages,
                    })
            }
            onPin={channel || menu.m.revokedAt ? undefined : (secs) => void pinMessage(menu.m, secs)}
            onForward={menu.m.revokedAt || menu.m.kind === "poll" ? undefined : () => setForward(menu.m)}
            onInfo={menu.m.fromMe && !channel ? () => setInfoFor(menu.m) : undefined}
            onClose={() => setMenu(null)}
          />
        )}
        {infoFor && (
          <NativeMessageInfo
            accountId={account.id}
            message={infoFor}
            connected={connected}
            onClose={() => setInfoFor(null)}
            onOpenChat={(ids) => onOpenChat(ids)}
          />
        )}
        {forward && (
          <NativeForwardDialog
            accountId={account.id}
            fromChatId={chatId}
            message={forward}
            onClose={() => setForward(null)}
            onError={onError}
          />
        )}
        {summary && (
          <NativeSummaryModal
            accountId={account.id}
            chatKey={prefsKey}
            chatId={chatId}
            chatName={name}
            messages={messages}
            canLoadOlder={moreStored || connected}
            onLoadOlder={loadOlder}
            onClose={() => setSummary(false)}
          />
        )}
      </div>
      {(profileId || info) && (
        <NativeInfoPanel
          key={profileId ?? chatId}
          accountId={account.id}
          chatId={profileId ?? chatId}
          connected={connected}
          picture={profileId ? null : picture}
          onOpenChat={onOpenChat}
          overlay={infoOverlay}
          onClose={() => {
            setProfileId(null);
            setInfo(false);
          }}
        />
      )}
    </>
  );
}

/** "Manually" read-receipt mode: sends the blue ticks for this chat when clicked. */
export function NativeReadReceiptButton({ accountId, chatId, messages }: { accountId: string; chatId: string; messages: NativeMessage[] }) {
  const [sentFor, setSentFor] = useState<string | null>(null); // id of the last incoming message we've acknowledged
  const lastIn = [...messages].reverse().find((m) => !m.fromMe);
  const pending = !!lastIn && sentFor !== lastIn.id;
  return (
    <Button
      variant={pending ? "primary" : "ghost"}
      size="sm"
      title={pending ? "Send read receipt (blue ticks) for this chat" : "Read receipt already sent"}
      disabled={!pending}
      onClick={async () => {
        try {
          await nativeWa.sendReceipt(accountId, chatId);
          setSentFor(lastIn!.id);
        } catch (e) {
          await confirm({ title: "Couldn't send read receipt", message: errMsg(e), confirmLabel: "OK" });
        }
      }}
    >
      <CheckCheck size={16} className={pending ? "" : "text-sky-500"} />
    </Button>
  );
}

/** Per-chat auto-translate: incoming shown in one language, your messages sent in another. */
export function AutoTranslateButton({ prefsKey }: { prefsKey: string }) {
  const value = useChatPrefs((s) => s.autoTranslate[prefsKey]);
  const setAutoTranslate = useChatPrefs((s) => s.setAutoTranslate);
  const [open, setOpen] = useState(false);
  if (!aiConfigured()) return null;
  const select = (label: string, which: "in" | "out") => (
    <label className="block space-y-1">
      <span className="text-xs text-neutral-500">{label}</span>
      <select
        value={value?.[which] ?? ""}
        onChange={(e) => setAutoTranslate(prefsKey, { [which]: e.target.value || undefined })}
        className="w-full rounded-lg border border-neutral-300 dark:border-neutral-700 bg-transparent px-2 py-1 text-sm outline-none"
      >
        <option value="">Off</option>
        {LANGUAGES.map(([c, n]) => (
          <option key={c} value={c}>
            {n}
          </option>
        ))}
      </select>
    </label>
  );
  const on = value?.in || value?.out;
  return (
    <Popover
      open={open}
      onClose={() => setOpen(false)}
      align="right"
      className="p-3 space-y-2 w-60"
      trigger={
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setOpen((v) => !v)}
          title={on ? `Auto-translate on${value?.in ? ` (incoming → ${langName(value.in)})` : ""}` : "Auto-translate"}
          className={cn(on && "text-wa-dark dark:text-wa")}
        >
          <Languages size={16} />
        </Button>
      }
    >
      <div className="text-xs font-medium">Auto-translate this chat</div>
      {select("Show incoming messages in", "in")}
      {select("Send my messages in", "out")}
    </Popover>
  );
}
