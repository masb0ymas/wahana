/** One message bubble in a conversation. */
import { memo, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Pin } from "lucide-react";
import { Avatar } from "@/components/ui";
import { cn, convKey, formatMessageTime } from "@/lib/utils";
import { WaMarkdown } from "@/lib/waMarkdown";
import { mentionResolver } from "@/lib/mentions";
import { nativeWa, type NativeMessage, type NativeReply } from "@/lib/nativeWa";
import { LinkPreviewCard } from "@/components/LinkPreview";
import { AckIcon, ImageNoteView, ReplySuggestView, TranslationView } from "@/components/MessageExtras";
import { useStoryJump } from "@/store/storyJump";
import { usePicture } from "@/screens/whatsapp/usePicture";
import { NativeMediaView } from "@/screens/whatsapp/NativeMediaView";
import { ContactCard, LocationCard, PollCard } from "@/screens/whatsapp/NativeInteractive";
import { bareId, summarize, useReactions } from "@/store/reactions";
import { useRevoked } from "@/store/revoked";
import { useWhatsApp } from "@/store/whatsapp";
import { secs } from "@/screens/whatsapp/shared";

/** The sender's round profile photo, loaded on demand and shared with the chat list. */
export function BubbleAvatar({
  accountId,
  chatId,
  connected,
  name,
  onClick,
}: {
  accountId: string;
  chatId: string | null;
  connected: boolean;
  name: string;
  onClick?: () => void;
}) {
  const picture = usePicture(accountId, chatId ?? "", connected && !!chatId);
  const avatar = <Avatar src={picture} name={name} size={28} />;
  return onClick ? (
    // `block`: an inline button sits on a text line, and a picture avatar's baseline (its bottom
    // edge) leaves the line's descent below it, lifting the avatar above the bubble's bottom.
    <button className="block rounded-full" onClick={onClick} title="Profile">
      {avatar}
    </button>
  ) : (
    avatar
  );
}

export const Bubble = memo(function Bubble({
  accountId,
  connected,
  message: m,
  album,
  showSender,
  avatar,
  avatarChatId,
  pinned,
  blurred,
  tail,
  onMenu,
  onPickReply,
  onJumpTo,
  onProfile,
  onOpenNumber,
}: {
  accountId: string;
  connected: boolean;
  message: NativeMessage;
  /** The photos and videos sent together with this one, when it leads an album. */
  album?: NativeMessage[];
  showSender: boolean;
  /** Their round profile photo beside the bubble: shown, kept as blank space, or absent. */
  avatar: "show" | "space" | "none";
  avatarChatId: string | null;
  pinned: boolean;
  /** Blur the bubble content until hovered (per-chat privacy). */
  blurred: boolean;
  /** Last bubble of a run from one speaker: draw WhatsApp's tail at its bottom corner. */
  tail: boolean;
  onMenu: (m: NativeMessage, pos: { x: number; y: number }) => void;
  /** Put a suggested reply into the composer. */
  onPickReply?: (text: string) => void;
  /** Open the quoted message: jump within this chat, or switch to the chat that stores it. */
  onJumpTo: (reply: NativeReply) => void;
  onProfile: (chatId: string) => void;
  /** Open (or start) the chat with a phone number, e.g. from a shared contact. */
  onOpenNumber: (phone: string) => void;
}) {
  const mine = m.fromMe;
  const sticker = m.media?.kind === "sticker";
  // Hooks run before the tombstone early return: a message can be deleted in place.
  const reactionMap = useReactions((s) => s.byMsg[bareId(m.id)]);
  const tomb = useRevoked((s) => s.items[`${convKey(accountId, m.chatId)}:${bareId(m.id)}`]);
  const [showEdits, setShowEdits] = useState(false);
  const [bodyOpen, setBodyOpen] = useState(false);
  // Group mentions are digits; the members list (shared with the composer) names them.
  const group = m.chatId.endsWith("@g.us");
  const me = useWhatsApp((s) => s.accounts.find((a) => a.id === accountId)?.me ?? null);
  const { data: groupInfo } = useQuery({
    queryKey: ["native-chat-info", accountId, m.chatId],
    queryFn: () => nativeWa.chatInfo(accountId, m.chatId),
    enabled: group && connected,
    staleTime: 5 * 60_000,
  });
  const mentionFor = useMemo(() => mentionResolver(groupInfo, me?.id), [groupInfo, me?.id]);
  // Deleted for everyone: the stored copy keeps what it said; older tombstones only know that it went.
  const revoked = m.revokedAt != null || (!!tomb && (tomb.kind ?? "revoked") === "revoked");
  // Stickers float without a bubble and a deleted message is a dashed outline: neither takes a tail.
  const hasTail = tail && !sticker && !revoked;
  // Deleted messages keep their full text (struck through); a long body is clamped with a Read more toggle.
  const longBody = !revoked && !!m.body && (m.body.length > 350 || m.body.split("\n").length > 6);
  // A channel reports totals only; mine comes from what I reacted locally.
  const reactions =
    m.channelReactions.length > 0
      ? m.channelReactions.map((r) => ({ emoji: r.emoji, count: r.count, me: reactionMap?.me === r.emoji }))
      : summarize(reactionMap, []);
  return (
    <div className={cn("flex items-end gap-1.5", mine ? "justify-end" : "justify-start")}>
      {!mine && avatar !== "none" && (
        // The row aligns to the bottom of the bubble column; reactions overhang the bubble by 12px
        // (h-5 row pulled up by -mt-2), so lift the avatar by the same to sit level with the bubble.
        <div className={cn("w-7 shrink-0", reactions.length > 0 && "mb-3")}>
          {avatar === "show" && (
            <BubbleAvatar
              accountId={accountId}
              chatId={avatarChatId}
              connected={connected}
              name={m.senderName || m.senderPhone || ""}
              onClick={avatarChatId ? () => onProfile(avatarChatId) : undefined}
            />
          )}
        </div>
      )}
      <div className={cn("flex flex-col max-w-[85%] @2xl:max-w-[70%]", mine ? "items-end" : "items-start")}>
        <div
          onContextMenu={(e) => {
            e.preventDefault();
            onMenu(m, { x: e.clientX, y: e.clientY });
          }}
          title="Right-click for more"
          className={cn(
            "relative rounded-lg px-3 pt-2 pb-1.5 text-sm selectable",
            hasTail && (mine ? "rounded-br-none" : "rounded-bl-none"),
            revoked
              ? cn(
                  "border border-dashed",
                  mine
                    ? "border-wa-dark/40 bg-[#d9fdd3]/40 dark:bg-wa-teal/30"
                    : "border-neutral-300 dark:border-neutral-700 bg-white/60 dark:bg-neutral-800/60",
                )
              : sticker
                ? "bg-transparent"
                : mine
                  ? "bg-[#d9fdd3] dark:bg-wa-teal text-neutral-900 dark:text-white shadow-sm"
                  : "bg-white dark:bg-neutral-800 shadow-sm",
            blurred && "blur-[3px] transform-gpu hover:blur-none transition-[filter] duration-150",
          )}
        >
          {hasTail && (
            // WhatsApp's own tail shape, flipped upside down to sit at the bottom corner: same fill
            // as the bubble, flush with its squared corner and pointing outward. Its path starts at
            // y=1 of 13, which the flip puts 1px above the bottom, hence the 1px drop.
            <svg
              aria-hidden
              viewBox="0 0 8 13"
              className={cn(
                "pointer-events-none absolute -bottom-px w-2 h-[13px] -scale-y-100",
                mine ? "-right-2 text-[#d9fdd3] dark:text-wa-teal" : "-left-2 text-white dark:text-neutral-800",
              )}
            >
              <path
                fill="currentColor"
                d={
                  mine
                    ? "M5.188 1H0v11.193l6.467-8.625C7.526 2.156 6.958 1 5.188 1z"
                    : "M1.533 3.568 8 12.193V1H2.812C1.042 1 .474 2.156 1.533 3.568z"
                }
              />
            </svg>
          )}
          {revoked && (
            <div className="flex items-center gap-1 text-xs italic text-neutral-500 dark:text-neutral-400 mb-0.5">
              🚫 {mine ? "You deleted this message" : "This message was deleted"}
              {m.revokedAt != null && <span className="not-italic text-[10px]">· {formatMessageTime(secs(m.revokedAt))}</span>}
            </div>
          )}
          {showSender && (m.senderName || m.senderPhone) && (
            <div className="flex items-baseline gap-1.5 text-[11px] mb-0.5">
              <button
                className="font-semibold text-wa-dark dark:text-wa hover:underline disabled:no-underline"
                disabled={!avatarChatId}
                onClick={() => avatarChatId && onProfile(avatarChatId)}
              >
                {m.senderName || m.senderPhone}
              </button>
              {m.senderPhone && m.senderName && m.senderName !== m.senderPhone && <span className="text-neutral-400">{m.senderPhone}</span>}
            </div>
          )}
          {pinned && (
            <div className="flex items-center gap-1 text-[10px] text-neutral-500 dark:text-neutral-300/70 mb-0.5">
              <Pin size={10} /> Pinned
            </div>
          )}
          {m.replyTo && (
            <button
              onClick={() => (m.replyTo!.status ? useStoryJump.getState().open(m.replyTo!.id) : onJumpTo(m.replyTo!))}
              className={cn(
                "mb-1 block w-full min-w-[140px] rounded-md border-l-4 border-wa-dark px-2 py-1 text-left text-xs",
                mine ? "bg-black/5 dark:bg-black/20" : "bg-neutral-100 dark:bg-neutral-700/60",
              )}
            >
              <div className="font-semibold text-wa-dark dark:text-wa truncate">
                {m.replyTo.fromMe ? "You" : m.replyTo.senderName || "Message"}
              </div>
              <div className="line-clamp-2 break-words text-neutral-600 dark:text-neutral-300">{m.replyTo.text || "Message"}</div>
            </button>
          )}
          {!revoked && m.statusMention && (
            <button
              onClick={() => useStoryJump.getState().open(m.statusMention!)}
              title="Open story"
              className={cn(
                "mb-1 flex w-full min-w-[160px] items-center gap-2 rounded-md border-l-4 border-[#ff8a65] px-2 py-1.5 text-left text-xs",
                mine ? "bg-black/5 dark:bg-black/20" : "bg-neutral-100 dark:bg-neutral-700/60",
              )}
            >
              <span className="text-base leading-none">📣</span>
              <span className="flex-1">
                <span className="block font-medium text-wa-dark dark:text-wa">Mentioned you in a story</span>
                <span className="block text-[10px] text-neutral-500 dark:text-neutral-400">Tap to view</span>
              </span>
            </button>
          )}
          <div className={cn(revoked && "opacity-60")}>
            {album ? (
              album.length <= 4 ? (
                <div className="mb-1 grid grid-cols-2 gap-0.5 w-[300px]">
                  {album.map((a) => (
                    <NativeMediaView key={a.id} accountId={accountId} message={a} connected={connected} tile />
                  ))}
                </div>
              ) : (
                <div className="hscroll mb-1 grid grid-rows-2 grid-flow-col gap-0.5 w-[300px] overflow-x-auto overflow-y-hidden [grid-auto-columns:calc(50%-1px)]">
                  {album.map((a) => (
                    <NativeMediaView key={a.id} accountId={accountId} message={a} connected={connected} tile />
                  ))}
                </div>
              )
            ) : m.media ? (
              <div className={cn(m.body && "mb-1")}>
                <NativeMediaView accountId={accountId} message={m} connected={connected} />
              </div>
            ) : m.kind === "poll" && m.interactive?.poll ? (
              <PollCard
                data={m.interactive.poll}
                onVote={
                  connected && m.interactive.poll.results?.canVote && !revoked
                    ? (options) => nativeWa.pollVote(accountId, m.chatId, m.id, options)
                    : undefined
                }
              />
            ) : m.kind === "location" && m.interactive?.location ? (
              <LocationCard data={m.interactive.location} />
            ) : m.kind === "contact" && m.interactive?.contacts ? (
              <ContactCard data={m.interactive.contacts} onOpen={onOpenNumber} />
            ) : m.kind === "media" || m.kind === "unsupported" ? (
              <div className="italic text-neutral-500 dark:text-neutral-400">
                {m.kind === "media" ? "📎 Media (not available for this older message)" : "Unsupported message"}
              </div>
            ) : null}
            {album?.slice(1).map((a) =>
              a.body ? (
                <div key={a.id} className="break-words">
                  <WaMarkdown text={a.body} mentions={group ? mentionFor : undefined} />
                </div>
              ) : null,
            )}
            {m.body && !m.media && !m.interactive && !revoked && <LinkPreviewCard message={m} />}
            {m.body && !m.statusMention && !m.interactive && (
              <div className={cn("break-words", revoked && "line-through decoration-neutral-400")}>
                <div className={cn(!bodyOpen && longBody && "line-clamp-6")}>
                  <WaMarkdown text={m.body} mentions={group ? mentionFor : undefined} />
                </div>
                {longBody && (
                  <button
                    onClick={() => setBodyOpen((v) => !v)}
                    className="mt-1 text-xs font-medium text-wa-dark dark:text-wa hover:underline"
                  >
                    {bodyOpen ? "Show less" : "Read more"}
                  </button>
                )}
              </div>
            )}
          </div>
          {showEdits && m.edits.length > 0 && (
            <div className="mt-1 space-y-1 border-l-2 border-neutral-300 dark:border-neutral-600 pl-2">
              {m.edits.map((e, i) => (
                <div key={i} className="text-xs text-neutral-500 dark:text-neutral-400">
                  <span className="line-through break-words">{e.body}</span>
                  <span className="ml-1 text-[10px]">· replaced {formatMessageTime(secs(e.replacedAt))}</span>
                </div>
              ))}
            </div>
          )}
          <TranslationView id={m.id} />
          <ImageNoteView id={m.id} />
          <ReplySuggestView id={m.id} onPick={onPickReply} />
          <div className="flex items-center justify-end gap-1 mt-0.5 text-[10px] text-neutral-500 dark:text-neutral-300/70">
            {m.editedAt != null &&
              (m.edits.length > 0 ? (
                <button
                  onClick={() => setShowEdits((v) => !v)}
                  title={showEdits ? "Hide earlier versions" : `Show ${m.edits.length} earlier version${m.edits.length > 1 ? "s" : ""}`}
                  className="italic underline decoration-dotted hover:text-neutral-700 dark:hover:text-neutral-200"
                >
                  edited
                </button>
              ) : (
                <span className="italic">edited</span>
              ))}
            {formatMessageTime(secs(m.timestamp))}
            {mine && <AckIcon ack={m.ack} />}
          </div>
        </div>
        {reactions.length > 0 && (
          <div className="-mt-2 h-5 mx-2 flex gap-1 z-10">
            {reactions.map((r) => (
              <span
                key={r.emoji}
                title={r.me ? "You reacted" : undefined}
                className={cn(
                  "rounded-full bg-white dark:bg-neutral-800 border px-1.5 py-px text-[12px] leading-4 shadow-sm",
                  r.me ? "border-wa-dark" : "border-neutral-200 dark:border-neutral-700",
                )}
              >
                {r.emoji}
                {r.count > 1 && <span className="ml-0.5 text-[10px] text-neutral-500">{r.count}</span>}
              </span>
            ))}
          </div>
        )}
      </div>
    </div>
  );
});
