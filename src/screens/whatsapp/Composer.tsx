/** The message composer: text, attachments, polls, replies and edits. */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Languages, Loader2, Paperclip, Pencil, Send, X, Reply } from "lucide-react";
import { Button } from "@/components/ui";
import { EmojiButton } from "@/components/EmojiPicker";
import { StickerButton } from "@/components/StickerPicker";
import { clipboardImage, nameClipboardFile, noteSticker } from "@/lib/stickers";
import { langName, translate } from "@/lib/ai";
import { formatBytes } from "@/lib/mediaCache";
import { cn, errMsg } from "@/lib/utils";
import { stripWaMarkdown } from "@/lib/waMarkdown";
import { applyMentions, memberLabel, type PickedMention } from "@/lib/mentions";
import { nativeWa, type NativeAccount, type NativeGroupMember, type NativeMessage } from "@/lib/nativeWa";
import { TranslateDraftButton, WriteAssistButton } from "@/components/DraftAssist";
import { QuickReplyPicker } from "@/components/QuickReplyPicker";
import { MentionPicker } from "@/components/MentionPicker";
import { NativeSmartReplies } from "@/screens/whatsapp/NativeAi";
import { cacheSentMedia } from "@/screens/whatsapp/NativeMediaView";
import { AttachMenu, PollDialog } from "@/components/AttachMenu";
import { readReceiptsFor, sendTypingFor } from "@/store/settings";
import { nativeAccountKey, nativeChatKey } from "@/lib/account";
import { useDrafts } from "@/store/drafts";
import { type Attachment } from "@/screens/whatsapp/shared";

export function Composer({
  account,
  chatId,
  chatName,
  messages,
  autoOut,
  picked,
  onPicked,
  onPick,
  onError,
  replyTo,
  replyContext,
  editing,
  onCancelReply,
  onCancelEdit,
  onSent,
  attachment,
  setAttachment,
  attach,
  compact,
}: {
  account: NativeAccount;
  chatId: string;
  chatName: string;
  messages: NativeMessage[];
  /** Language your messages are translated into before sending, if set for this chat. */
  autoOut: string | undefined;
  /** A suggested reply chosen above the composer. */
  picked: string | null;
  onPicked: () => void;
  onPick: (text: string) => void;
  onError: (e: string) => void;
  /** The message being replied to, quoted when the text is sent. */
  replyTo: NativeMessage | null;
  /** Source chat when `replyTo` lives in another chat (reply privately); null for a same-chat reply. */
  replyContext: { chatId: string; chatName: string } | null;
  /** The message being edited; sending replaces its text instead of a new message. */
  editing: NativeMessage | null;
  onCancelReply: () => void;
  onCancelEdit: () => void;
  /** Pull the view to the newest message after sending one. */
  onSent: () => void;
  /** The file waiting to be sent, owned by the conversation so a drop anywhere attaches it. */
  attachment: Attachment | null;
  setAttachment: (attachment: Attachment | null) => void;
  /** Queue a file as the pending attachment (dropped, picked or pasted). */
  attach: (file: File) => void;
  /** Stack the tools above the input, for a narrow container (e.g. a grid tile). */
  compact?: boolean;
}) {
  // Unsent text is kept per chat, so switching away and back finds it again. Text loaded
  // for an edit is not a draft and is never saved.
  const draftKey = nativeChatKey(account.id, chatId);
  const [text, setTextRaw] = useState(() => useDrafts.getState().drafts[draftKey] ?? "");
  const editingRef = useRef(editing);
  editingRef.current = editing;
  const setText = useCallback(
    (v: string) => {
      setTextRaw(v);
      if (!editingRef.current) useDrafts.getState().set(draftKey, v);
    },
    [draftKey],
  );
  const [sending, setSending] = useState(false);
  const [pollOpen, setPollOpen] = useState(false);
  // Polls go to people and groups; channels and stories take none.
  const canPoll = !chatId.endsWith("@newsletter") && chatId !== "status@broadcast";
  const [slash, setSlash] = useState<string | null>(null); // "/query" at the start of the composer
  const [mention, setMention] = useState<{ query: string; start: number } | null>(null); // "@query" before the caret
  const pickedMentions = useRef<PickedMention[]>([]);
  const taRef = useRef<HTMLTextAreaElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const connected = account.status === "working";
  const group = chatId.endsWith("@g.us");
  // Shared key with the message bubbles, so group members are fetched once per chat.
  const { data: chatDetails } = useQuery({
    queryKey: ["native-chat-info", account.id, chatId],
    queryFn: () => nativeWa.chatInfo(account.id, chatId),
    enabled: group && connected,
    staleTime: 5 * 60_000,
  });
  const members = useMemo(() => (chatDetails?.type === "group" ? chatDetails.members.filter((m) => !m.isMe) : []), [chatDetails]);

  // Typing presence: composing at most every 4s while typing, paused after 5s idle.
  const typingRef = useRef<{ last: number; timer?: ReturnType<typeof setTimeout> }>({ last: 0 });
  const stopTyping = () => {
    clearTimeout(typingRef.current.timer);
    if (typingRef.current.last) {
      typingRef.current.last = 0;
      void nativeWa.setTyping(account.id, chatId, false).catch(() => {});
    }
  };
  const noteTyping = () => {
    if (!sendTypingFor(nativeAccountKey(account.id))) return;
    const now = Date.now();
    if (now - typingRef.current.last > 4000) {
      typingRef.current.last = now;
      void nativeWa.setTyping(account.id, chatId, true).catch(() => {});
    }
    clearTimeout(typingRef.current.timer);
    typingRef.current.timer = setTimeout(stopTyping, 5000);
  };
  useEffect(() => stopTyping, [chatId]); // eslint-disable-line react-hooks/exhaustive-deps

  // A picked mention only means something in the chat it was inserted in.
  useEffect(() => {
    setMention(null);
    pickedMentions.current = [];
  }, [chatId]);

  const syncMention = useCallback((value: string, caret: number) => {
    const m = value.slice(0, caret).match(/(?:^|\s)@([^\s@]*)$/);
    setMention(m ? { query: m[1]!, start: caret - m[1]!.length - 1 } : null);
  }, []);

  const pickMention = (member: NativeGroupMember) => {
    const ta = taRef.current;
    if (!ta || !mention) return;
    const caret = ta.selectionStart ?? text.length;
    const label = memberLabel(member);
    const insert = `@${label} `;
    setText(text.slice(0, mention.start) + insert + text.slice(caret));
    if (!pickedMentions.current.some((p) => p.jid === member.id)) pickedMentions.current.push({ label, jid: member.id });
    setMention(null);
    requestAnimationFrame(() => {
      ta.focus();
      ta.selectionStart = ta.selectionEnd = mention.start + insert.length;
    });
  };

  useEffect(() => {
    taRef.current?.focus();
  }, [chatId]);

  // Choosing Reply puts the cursor in the composer.
  useEffect(() => {
    if (!replyTo) return;
    const frame = requestAnimationFrame(() => taRef.current?.focus());
    return () => cancelAnimationFrame(frame);
  }, [replyTo]);

  useEffect(() => {
    if (picked === null) return;
    setText(picked);
    onPicked();
    taRef.current?.focus();
  }, [picked, onPicked, setText]);

  // Editing loads the message's text into the composer; finishing or cancelling the edit
  // brings back whatever draft was there before.
  const draftBeforeEdit = useRef<string | null>(null);
  useEffect(() => {
    if (editing) {
      draftBeforeEdit.current ??= text;
      setText(editing.body);
      taRef.current?.focus();
    } else if (draftBeforeEdit.current !== null) {
      setText(draftBeforeEdit.current);
      draftBeforeEdit.current = null;
    }
  }, [editing]); // eslint-disable-line react-hooks/exhaustive-deps

  // Keep the textarea's height in step with programmatic changes (send, emoji, AI rewrite).
  useEffect(() => {
    const el = taRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = Math.min(el.scrollHeight, 160) + "px";
  }, [text]);

  const sendSticker = async (webp: Blob) => {
    if (sending || !connected) return;
    setSending(true);
    onSent();
    try {
      const sent = await nativeWa.sendMedia(account.id, chatId, webp, "sticker.webp", "", replyTo?.id ?? null, true);
      void cacheSentMedia(account.id, sent, webp);
      noteSticker(webp);
      onCancelReply();
    } catch (e) {
      onError(errMsg(e));
    } finally {
      setSending(false);
    }
  };

  // Pasting a screenshot works wherever focus is, not only inside the text box.
  const attachRef = useRef(attach);
  attachRef.current = attach;
  useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && t !== taRef.current && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
      const img = clipboardImage(e.clipboardData);
      if (!img) return;
      e.preventDefault();
      attachRef.current(nameClipboardFile(img));
      taRef.current?.focus();
    };
    document.addEventListener("paste", onPaste);
    return () => document.removeEventListener("paste", onPaste);
  }, []);

  // A cross-chat reply (reply privately) quotes the source chat, not this one.
  const quoteChat = replyContext && replyContext.chatId !== chatId ? replyContext.chatId : null;

  const submit = async () => {
    const draft = text.trim();
    if ((!draft && !attachment) || sending || !connected) return;
    stopTyping();
    // A new message always belongs at the bottom; editing stays where the message is.
    if (!editing) onSent();
    // "Only when I reply": the receipt goes out right before our message.
    if (readReceiptsFor(nativeAccountKey(account.id)) === "on-reply") void nativeWa.sendReceipt(account.id, chatId).catch(() => {});
    setSending(true);
    try {
      const converted = applyMentions(draft, pickedMentions.current);
      const body = converted.text && autoOut && !editing ? await translate(converted.text, autoOut) : converted.text;
      if (editing) {
        await nativeWa.edit(account.id, chatId, editing.id, body);
        onCancelEdit();
      } else if (attachment) {
        const sent = await nativeWa.sendMedia(
          account.id,
          chatId,
          attachment.file,
          attachment.file.name,
          body,
          replyTo?.id ?? null,
          false,
          converted.mentions,
          quoteChat,
        );
        void cacheSentMedia(account.id, sent, attachment.file);
        setAttachment(null);
        onCancelReply();
      } else {
        await nativeWa.sendText(account.id, chatId, body, replyTo?.id ?? null, converted.mentions, quoteChat);
        onCancelReply();
      }
      pickedMentions.current = [];
      setText("");
    } catch (e) {
      onError(errMsg(e));
    } finally {
      setSending(false);
      taRef.current?.focus();
    }
  };

  return (
    <div
      className={cn(
        "relative shrink-0 bg-white dark:bg-neutral-900 border-t border-neutral-200 dark:border-neutral-800",
        compact ? "p-2 space-y-1.5" : "p-3 space-y-2",
      )}
    >
      {slash !== null && (
        <QuickReplyPicker
          account={nativeAccountKey(account.id)}
          query={slash}
          ctx={{ name: chatName, phone: chatId.endsWith("@s.whatsapp.net") ? `+${chatId.split("@")[0]}` : "" }}
          onClose={() => setSlash(null)}
          onPick={(t) => {
            setText(t);
            setSlash(null);
            requestAnimationFrame(() => taRef.current?.focus());
          }}
        />
      )}
      {slash === null && mention && members.length > 0 && (
        <MentionPicker query={mention.query} members={members} onPick={pickMention} onClose={() => setMention(null)} />
      )}
      {!editing && <NativeSmartReplies accountId={account.id} chatId={chatId} chatName={chatName} messages={messages} onPick={onPick} />}
      {editing ? (
        <div className="flex items-start gap-2 rounded-lg bg-neutral-100 dark:bg-neutral-800 px-2 py-1.5 text-xs">
          <Pencil size={14} className="mt-0.5 text-neutral-500" />
          <div className="min-w-0 flex-1">
            <div className="font-medium text-wa-dark dark:text-wa">Editing message</div>
            <div className="truncate text-neutral-500">{stripWaMarkdown(editing.body) || "📎 Media"}</div>
          </div>
          <button onClick={onCancelEdit} title="Cancel edit" className="p-1 text-neutral-500 hover:text-neutral-800">
            <X size={14} />
          </button>
        </div>
      ) : replyTo ? (
        <div className="flex items-start gap-2 rounded-lg bg-neutral-100 dark:bg-neutral-800 px-2 py-1.5 text-xs">
          <Reply size={14} className="mt-0.5 text-neutral-500" />
          <div className="min-w-0 flex-1">
            <div className="font-medium text-wa-dark dark:text-wa">
              {replyTo.fromMe ? "You" : replyTo.senderName || "Them"}
              {quoteChat && replyContext && <span className="font-normal text-neutral-500"> · {replyContext.chatName}</span>}
            </div>
            <div className="truncate text-neutral-500">{stripWaMarkdown(replyTo.body) || "📎 Media"}</div>
          </div>
          <button onClick={onCancelReply} title="Cancel reply" className="p-1 text-neutral-500 hover:text-neutral-800">
            <X size={14} />
          </button>
        </div>
      ) : null}
      {!editing && attachment && (
        <div className="flex items-center gap-2 rounded-lg bg-neutral-100 dark:bg-neutral-800 px-2 py-1.5 text-xs">
          {attachment.preview ? (
            <img src={attachment.preview} alt="" className="w-10 h-10 rounded object-cover" />
          ) : (
            <Paperclip size={16} className="text-neutral-500" />
          )}
          <div className="min-w-0 flex-1">
            <div className="font-medium truncate">{attachment.file.name}</div>
            <div className="text-neutral-500">{formatBytes(attachment.file.size)} · the text below is sent as its caption</div>
          </div>
          <button onClick={() => setAttachment(null)} title="Remove attachment" className="p-1 text-neutral-500 hover:text-neutral-800">
            <X size={14} />
          </button>
        </div>
      )}
      {autoOut && (
        <div className="text-[11px] text-neutral-500 flex items-center gap-1">
          <Languages size={12} /> Your messages are translated to {langName(autoOut)} before sending
        </div>
      )}
      <div className={cn("flex gap-1", compact ? "flex-col" : "items-end")}>
        <div className="flex items-center gap-1">
          <input
            ref={fileRef}
            type="file"
            hidden
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (f) attach(f);
              e.target.value = "";
            }}
          />
          <AttachMenu
            disabled={!connected || !!editing}
            kinds={canPoll ? ["image", "file", "poll"] : ["image", "file"]}
            onPick={(k) => {
              if (k === "poll") return setPollOpen(true);
              const input = fileRef.current;
              if (!input) return;
              input.accept = k === "image" ? "image/*,video/*" : "";
              input.click();
            }}
          />
          {pollOpen && (
            <PollDialog
              onClose={() => setPollOpen(false)}
              onSend={async (question, options, multiple) => {
                await nativeWa.sendPoll(account.id, chatId, question, options, multiple);
                onSent();
              }}
            />
          )}
          <TranslateDraftButton text={text} onResult={setText} />
          <WriteAssistButton text={text} account={nativeAccountKey(account.id)} onResult={setText} />
          <StickerButton onPick={sendSticker} disabled={!connected || !!editing || sending} />
          <EmojiButton
            onPick={(emoji) => {
              const ta = taRef.current;
              const start = ta?.selectionStart ?? text.length;
              const end = ta?.selectionEnd ?? text.length;
              setText(text.slice(0, start) + emoji + text.slice(end));
              requestAnimationFrame(() => {
                if (!ta) return;
                ta.focus();
                ta.selectionStart = ta.selectionEnd = start + emoji.length;
              });
            }}
          />
        </div>
        <div className="flex items-end gap-1 min-w-0 flex-1">
          <textarea
            ref={taRef}
            value={text}
            onChange={(e) => {
              const v = e.target.value;
              setText(v);
              noteTyping();
              const sl = v.match(/^\/(\S*)$/);
              setSlash(sl ? sl[1]! : null);
              syncMention(v, e.target.selectionStart ?? v.length);
            }}
            onKeyUp={(e) => {
              if (e.key === "Enter" || e.key === "Tab" || e.key === "Escape") return;
              syncMention(e.currentTarget.value, e.currentTarget.selectionStart ?? 0);
            }}
            onClick={(e) => syncMention(e.currentTarget.value, e.currentTarget.selectionStart ?? 0)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                void submit();
              }
            }}
            rows={1}
            disabled={!connected}
            placeholder={
              !connected
                ? "Connect the account to send messages"
                : attachment
                  ? "Add a caption (optional)"
                  : compact
                    ? "Type a message"
                    : "Type a message (Enter to send, Shift+Enter for newline)"
            }
            className="flex-1 resize-none rounded-lg bg-neutral-100 dark:bg-neutral-800 px-3 py-2 text-sm outline-none max-h-40 disabled:opacity-60"
          />
          <Button onClick={submit} disabled={(!text.trim() && !attachment) || sending || !connected} title="Send">
            {sending ? <Loader2 size={16} className="animate-spin" /> : autoOut ? <Languages size={16} /> : <Send size={16} />}
          </Button>
        </div>
      </div>
    </div>
  );
}
