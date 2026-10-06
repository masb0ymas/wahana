import { useEffect, useMemo, useRef, useState } from "react";
import {
  CheckSquare,
  Eye,
  EyeOff,
  FileText,
  HardDrive,
  Image as ImageIcon,
  Loader2,
  Music,
  Play,
  RefreshCw,
  Square,
  Trash2,
  Video,
  X,
} from "lucide-react";
import { save } from "@tauri-apps/plugin-dialog";
import { writeFile } from "@tauri-apps/plugin-fs";
import { confirm } from "@/components/Confirm";
import { Lightbox, type LightboxItem } from "@/components/Lightbox";
import { Button } from "@/components/ui";
import { cacheDelete, cacheList, cacheRead, formatBytes, type CacheEntry } from "@/lib/mediaCache";
import { nativeWa } from "@/lib/nativeWa";
import { useMediaBlur } from "@/store/mediaBlur";
import { cn, displayId, errMsg } from "@/lib/utils";
import { useWhatsApp } from "@/store/whatsapp";

/**
 * Everything in the on-disk media cache, by account (session) and type, with sizes, and
 * the means to delete it. Files cached before metadata was kept have no account or chat,
 * so they are listed under "Unknown". Files of an account that has since been removed keep
 * their own "removed" session, so its media can be cleared in one go.
 */

type Kind = "image" | "video" | "audio" | "document";
type KindFilter = Kind | "all";
type Sort = "newest" | "largest";

const UNKNOWN = "__unknown";
const ALL = "__all";

const KINDS: { id: KindFilter; label: string; icon: typeof ImageIcon }[] = [
  { id: "all", label: "All", icon: HardDrive },
  { id: "image", label: "Images", icon: ImageIcon },
  { id: "video", label: "Videos", icon: Video },
  { id: "audio", label: "Audio", icon: Music },
  { id: "document", label: "Documents", icon: FileText },
];

const IMAGE_EXT = ["jpg", "jpeg", "png", "gif", "webp", "heic", "bmp"];
const VIDEO_EXT = ["mp4", "3gpp", "webm", "quicktime", "mov", "mkv"];
const AUDIO_EXT = ["ogg", "opus", "mp3", "mpeg", "m4a", "aac", "wav", "amr"];

function kindOf(e: CacheEntry): Kind {
  const k = e.meta?.kind;
  if (k === "image" || k === "sticker") return "image";
  if (k === "video") return "video";
  if (k === "audio" || k === "ptt") return "audio";
  if (k === "document") return "document";
  const ext = e.file.split(".").pop()?.toLowerCase() ?? "";
  if (IMAGE_EXT.includes(ext)) return "image";
  if (VIDEO_EXT.includes(ext)) return "video";
  if (AUDIO_EXT.includes(ext)) return "audio";
  return "document";
}

const mimeOf = (e: CacheEntry, kind: Kind) => {
  if (e.meta?.mimetype) return e.meta.mimetype;
  const ext = e.file.split(".").pop() ?? "bin";
  if (kind === "image") return `image/${ext === "jpg" ? "jpeg" : ext}`;
  if (kind === "video") return `video/${ext}`;
  if (kind === "audio") return `audio/${ext}`;
  return "application/octet-stream";
};

const fileNameOf = (e: CacheEntry) => e.meta?.fileName ?? `wahana-${e.file}`;

async function entryBlob(e: CacheEntry) {
  return new Blob([await cacheRead(e.file)], { type: mimeOf(e, kindOf(e)) });
}

/** Videos up to this size get a real frame as their thumbnail; bigger ones an icon. */
const VIDEO_THUMB_MAX = 25 * 1024 * 1024;

interface Row extends CacheEntry {
  kind: Kind;
  session: string;
}

export function MediaScreen() {
  const accounts = useWhatsApp((s) => s.accounts);
  const [entries, setEntries] = useState<CacheEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [session, setSession] = useState<string>(ALL);
  const [kind, setKind] = useState<KindFilter>("all");
  const [sort, setSort] = useState<Sort>("newest");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  // Page-wide override, persisted: once revealed, tiles stay unblurred until "Blur all" is clicked.
  const revealAll = useMediaBlur((s) => s.revealAll);
  const setRevealAll = useMediaBlur((s) => s.setRevealAll);
  const [chatNames, setChatNames] = useState<Record<string, string>>({});
  const [viewer, setViewer] = useState<LightboxItem | null>(null);
  const [playing, setPlaying] = useState<{ file: string; url: string; title: string } | null>(null);
  const [menu, setMenu] = useState<{ id: string; x: number; y: number } | null>(null);

  const refresh = async () => {
    try {
      setError(null);
      setEntries(await cacheList());
    } catch (e) {
      setError(errMsg(e));
    }
  };
  useEffect(() => {
    void refresh();
  }, []);

  // Chat names per account, to label files by the chat they came from.
  const accountIds = accounts.map((a) => a.id).join(",");
  useEffect(() => {
    let cancelled = false;
    void Promise.all(
      accounts.map((a) =>
        nativeWa
          .chats(a.id)
          .then((list) => list.map((c) => [`${a.id}|${c.id}`, c.name] as const))
          .catch(() => []),
      ),
    ).then((lists) => {
      if (!cancelled) setChatNames(Object.fromEntries(lists.flat()));
    });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [accountIds]);

  const chatLabel = (e: CacheEntry) => {
    if (!e.meta) return "";
    if (e.meta.chatId === "status@broadcast") return e.meta.fromMe ? "My status" : `Status · ${e.meta.sender}`;
    return chatNames[`${e.meta.account}|${e.meta.chatId}`] || displayId(e.meta.chatId);
  };

  const rows: Row[] = useMemo(
    () =>
      (entries ?? []).map((e) => ({
        ...e,
        kind: kindOf(e),
        session: e.meta?.account || UNKNOWN,
      })),
    [entries],
  );

  const sumOf = (list: Row[]) => list.reduce((s, r) => s + r.bytes, 0);
  const sessions = [
    { id: ALL, name: "All sessions", removed: false, rows },
    ...accounts.map((a) => ({ id: a.id, name: a.name, removed: false, rows: rows.filter((r) => r.session === a.id) })),
    // Accounts that are gone but still have media here.
    ...[...new Set(rows.map((r) => r.session))]
      .filter((id) => id !== UNKNOWN && !accounts.some((a) => a.id === id))
      .map((id) => {
        const own = rows.filter((r) => r.session === id);
        const name = own.find((r) => r.meta?.accountName)?.meta?.accountName ?? `Session ${id.slice(0, 8)}`;
        return { id, name, removed: true, rows: own };
      }),
    { id: UNKNOWN, name: "Unknown", removed: false, rows: rows.filter((r) => r.session === UNKNOWN) },
  ].filter((s) => s.id !== UNKNOWN || s.rows.length > 0);

  const inSession = session === ALL ? rows : rows.filter((r) => r.session === session);
  const shown = useMemo(() => {
    const list = kind === "all" ? inSession : inSession.filter((r) => r.kind === kind);
    return [...list].sort((a, b) => (sort === "largest" ? b.bytes - a.bytes : b.saved - a.saved));
  }, [inSession, kind, sort]);

  // A session that disappeared (account removed, or its last file deleted) falls back to all.
  useEffect(() => {
    if (!sessions.some((s) => s.id === session)) setSession(ALL);
  }, [sessions, session]);

  const selectedRows = shown.filter((r) => selected.has(r.file));
  const toggle = (file: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(file)) next.delete(file);
      else next.add(file);
      return next;
    });

  const remove = async (list: Row[], scope?: string) => {
    if (!list.length) return;
    const title = scope
      ? `Delete all ${list.length} media files of "${scope}" (${formatBytes(sumOf(list))}) from this computer?`
      : list.length === 1
        ? `Delete this file (${formatBytes(list[0]!.bytes)}) from this computer?`
        : `Delete ${list.length} files (${formatBytes(sumOf(list))}) from this computer?`;
    if (
      !(await confirm({
        title,
        message: "They stay in WhatsApp and are downloaded again when you open them.",
        danger: true,
        confirmLabel: "Delete",
      }))
    )
      return;
    setBusy(true);
    try {
      await cacheDelete(list.map((r) => r.file));
      if (playing && list.some((r) => r.file === playing.file)) stopAudio();
      setSelected(new Set());
      await refresh();
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(false);
    }
  };

  const stopAudio = () => {
    if (playing) URL.revokeObjectURL(playing.url);
    setPlaying(null);
  };

  const open = async (r: Row) => {
    try {
      if (r.kind === "document") {
        const path = await save({ defaultPath: fileNameOf(r) });
        if (path) await writeFile(path, new Uint8Array(await cacheRead(r.file)));
        return;
      }
      const blob = await entryBlob(r);
      const url = URL.createObjectURL(blob);
      if (r.kind === "audio") {
        stopAudio();
        setPlaying({ file: r.file, url, title: [chatLabel(r), formatBytes(r.bytes)].filter(Boolean).join(" · ") });
      } else {
        setViewer({ blobUrl: url, kind: r.kind, filename: fileNameOf(r) });
      }
    } catch (e) {
      setError(errMsg(e));
    }
  };

  const closeViewer = () => {
    if (viewer) URL.revokeObjectURL(viewer.blobUrl);
    setViewer(null);
  };

  const countOf = (k: KindFilter) => (k === "all" ? inSession : inSession.filter((r) => r.kind === k));

  return (
    <div className="flex-1 min-w-0 flex">
      <aside className="w-60 shrink-0 border-r border-neutral-200 dark:border-neutral-800 flex flex-col">
        <div className="px-4 pt-4 pb-2 text-xs font-semibold uppercase tracking-wide text-neutral-500">Sessions</div>
        <div className="flex-1 overflow-y-auto px-2 pb-3 space-y-0.5">
          {sessions.map((s) => (
            <button
              key={s.id}
              onClick={() => {
                setSession(s.id);
                setSelected(new Set());
              }}
              onContextMenu={(e) => {
                e.preventDefault();
                setMenu({ id: s.id, x: e.clientX, y: e.clientY });
              }}
              className={cn(
                "w-full text-left px-3 py-2 rounded-lg transition",
                session === s.id ? "bg-wa-dark/10 text-wa-dark" : "hover:bg-neutral-100 dark:hover:bg-neutral-800",
              )}
            >
              <span className="flex items-center gap-1.5 text-sm font-medium">
                <span className="truncate">{s.name}</span>
                {s.removed && (
                  <span className="shrink-0 rounded px-1 text-[10px] font-normal bg-neutral-200 text-neutral-600 dark:bg-neutral-700 dark:text-neutral-300">
                    removed
                  </span>
                )}
              </span>
              <span className="block text-xs text-neutral-500">
                {formatBytes(sumOf(s.rows))} · {s.rows.length} file{s.rows.length === 1 ? "" : "s"}
              </span>
            </button>
          ))}
        </div>
      </aside>

      <div className="flex-1 min-w-0 flex flex-col">
        <nav className="shrink-0 flex items-center gap-1 px-4 pt-3 border-b border-neutral-200 dark:border-neutral-800">
          {KINDS.map((k) => {
            const list = countOf(k.id);
            if (k.id !== "all" && k.id !== "image" && k.id !== "video" && list.length === 0) return null;
            return (
              <button
                key={k.id}
                onClick={() => {
                  setKind(k.id);
                  setSelected(new Set());
                }}
                className={cn(
                  "flex items-center gap-2 px-3 py-2 text-sm rounded-t-lg border-b-2 -mb-px transition",
                  kind === k.id
                    ? "border-wa-dark text-wa-dark font-medium"
                    : "border-transparent text-neutral-500 hover:text-neutral-800 dark:hover:text-neutral-200",
                )}
              >
                <k.icon size={16} />
                {k.label}
                <span className="text-xs text-neutral-400">
                  {list.length} · {formatBytes(sumOf(list))}
                </span>
              </button>
            );
          })}
        </nav>

        <div className="shrink-0 flex items-center gap-2 px-4 py-2 border-b border-neutral-200 dark:border-neutral-800 text-sm">
          {selectedRows.length > 0 ? (
            <>
              <span className="font-medium">
                {selectedRows.length} selected · {formatBytes(sumOf(selectedRows))}
              </span>
              <Button size="sm" variant="ghost" onClick={() => setSelected(new Set())}>
                <X size={12} /> Clear
              </Button>
              <span className="flex-1" />
              <Button size="sm" variant="danger" disabled={busy} onClick={() => remove(selectedRows)}>
                {busy ? <Loader2 size={12} className="animate-spin" /> : <Trash2 size={12} />} Delete selected
              </Button>
            </>
          ) : (
            <>
              <span className="text-neutral-500">
                {shown.length} file{shown.length === 1 ? "" : "s"} · {formatBytes(sumOf(shown))}
              </span>
              <span className="flex-1" />
              <select
                value={sort}
                onChange={(e) => setSort(e.target.value as Sort)}
                className="rounded-lg border border-neutral-300 bg-white px-2 py-1 text-xs dark:border-neutral-700 dark:bg-neutral-900"
              >
                <option value="newest">Newest first</option>
                <option value="largest">Largest first</option>
              </select>
              <Button
                size="sm"
                variant="ghost"
                disabled={!shown.length && !revealAll}
                onClick={() => setRevealAll(!revealAll)}
                title={revealAll ? "Blur all media again" : "Show all media without blur"}
              >
                {revealAll ? <EyeOff size={12} /> : <Eye size={12} />} {revealAll ? "Blur all" : "Reveal all"}
              </Button>
              <Button size="sm" variant="ghost" disabled={!shown.length} onClick={() => setSelected(new Set(shown.map((r) => r.file)))}>
                <CheckSquare size={12} /> Select all
              </Button>
              <Button size="sm" variant="ghost" onClick={refresh} title="Refresh">
                <RefreshCw size={12} />
              </Button>
              <Button size="sm" variant="danger" disabled={busy || !shown.length} onClick={() => remove(shown)}>
                <Trash2 size={12} /> Delete all shown
              </Button>
            </>
          )}
        </div>

        {error && <div className="shrink-0 px-4 py-2 text-xs text-red-600 selectable">{error}</div>}

        <div className="flex-1 min-h-0 overflow-y-auto p-4">
          {entries === null ? (
            <div className="h-full grid place-items-center text-neutral-500">
              <Loader2 className="animate-spin" />
            </div>
          ) : shown.length === 0 ? (
            <div className="h-full grid place-items-center text-sm text-neutral-500 text-center">
              <div>
                <HardDrive className="mx-auto mb-2 opacity-50" />
                No media saved on this computer here yet.
              </div>
            </div>
          ) : (
            <div className="grid gap-2 grid-cols-[repeat(auto-fill,minmax(140px,1fr))]">
              {shown.map((r) => (
                <Tile
                  key={r.file}
                  row={r}
                  label={chatLabel(r)}
                  selected={selected.has(r.file)}
                  selecting={selectedRows.length > 0}
                  playing={playing?.file === r.file}
                  revealAll={revealAll}
                  onToggle={() => toggle(r.file)}
                  onOpen={() => open(r)}
                  onDelete={() => remove([r])}
                />
              ))}
            </div>
          )}
        </div>

        {playing && (
          <div className="shrink-0 flex items-center gap-3 px-4 py-2 border-t border-neutral-200 dark:border-neutral-800">
            <Music size={16} className="shrink-0 text-wa-dark" />
            <span className="text-sm truncate max-w-[30%]">{playing.title}</span>
            <audio src={playing.url} controls autoPlay className="flex-1 h-8" />
            <button onClick={stopAudio} title="Close">
              <X size={16} />
            </button>
          </div>
        )}
      </div>

      {viewer && <Lightbox item={viewer} onClose={closeViewer} />}
      {menu &&
        (() => {
          const s = sessions.find((x) => x.id === menu.id);
          if (!s) return null;
          const close = () => setMenu(null);
          return (
            <div
              className="fixed inset-0 z-40"
              onClick={close}
              onContextMenu={(e) => {
                e.preventDefault();
                close();
              }}
            >
              <div
                className="absolute w-56 rounded-lg border border-neutral-200 dark:border-neutral-700 bg-white dark:bg-neutral-900 shadow-xl py-1 text-xs"
                style={{ left: Math.min(menu.x, window.innerWidth - 232), top: Math.min(menu.y, window.innerHeight - 60) }}
                onClick={(e) => e.stopPropagation()}
              >
                <button
                  disabled={busy || !s.rows.length}
                  className="w-full flex items-center gap-2 px-3 py-1.5 text-left text-red-600 hover:bg-neutral-100 dark:hover:bg-neutral-800 disabled:opacity-40"
                  onClick={() => {
                    close();
                    void remove(s.rows, s.name);
                  }}
                >
                  <Trash2 size={13} /> Delete all media ({formatBytes(sumOf(s.rows))})
                </button>
              </div>
            </div>
          );
        })()}
    </div>
  );
}

function Tile({
  row: r,
  label,
  selected,
  selecting,
  playing,
  revealAll,
  onToggle,
  onOpen,
  onDelete,
}: {
  row: Row;
  label: string;
  selected: boolean;
  selecting: boolean;
  playing: boolean;
  revealAll: boolean;
  onToggle: () => void;
  onOpen: () => void;
  onDelete: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false);
  const [thumb, setThumb] = useState<string | null>(null);
  const wantsThumb = r.kind === "image" || (r.kind === "video" && r.bytes <= VIDEO_THUMB_MAX);
  // Blurred by default; the page's "Reveal all" toggle clears every tile until it is blurred again.
  const blurred = !revealAll;

  // Load the bytes only once the tile scrolls into view.
  useEffect(() => {
    const el = ref.current;
    if (!el || !wantsThumb) return;
    const io = new IntersectionObserver(([e]) => e?.isIntersecting && setVisible(true), { rootMargin: "200px" });
    io.observe(el);
    return () => io.disconnect();
  }, [wantsThumb]);

  useEffect(() => {
    if (!visible) return;
    let cancelled = false;
    let url: string | null = null;
    entryBlob(r)
      .then((b) => {
        if (cancelled) return;
        url = URL.createObjectURL(b);
        setThumb(url);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      if (url) URL.revokeObjectURL(url);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visible, r.file]);

  const Icon = r.kind === "video" ? Video : r.kind === "audio" ? Music : r.kind === "image" ? ImageIcon : FileText;
  const date = new Date(r.meta?.timestamp || r.saved).toLocaleString();
  const name = r.kind === "document" || r.kind === "audio" ? r.meta?.fileName : null;
  const blurCls =
    blurred && (r.kind === "image" || r.kind === "video")
      ? "blur-md scale-105 transform-gpu group-hover:blur-none transition-[filter] duration-150"
      : "";

  return (
    <div
      ref={ref}
      title={[r.meta?.fileName, label, date, formatBytes(r.bytes)].filter(Boolean).join("\n")}
      className={cn(
        "group relative aspect-square rounded-lg overflow-hidden bg-neutral-100 dark:bg-neutral-800 cursor-pointer ring-2 ring-transparent transition",
        selected && "ring-wa-dark",
        playing && "ring-wa",
      )}
      onClick={selecting ? onToggle : onOpen}
    >
      {thumb && r.kind === "image" && <img src={thumb} alt="" className={cn("absolute inset-0 w-full h-full object-cover", blurCls)} />}
      {thumb && r.kind === "video" && (
        <video src={`${thumb}#t=0.1`} muted preload="metadata" className={cn("absolute inset-0 w-full h-full object-cover", blurCls)} />
      )}
      {!thumb && (
        <div className="absolute inset-0 grid place-items-center text-neutral-400">
          <div className="flex flex-col items-center gap-1 px-2 text-center">
            <Icon size={28} />
            {name && <span className="text-[11px] leading-tight line-clamp-2 break-all">{name}</span>}
          </div>
        </div>
      )}
      {r.kind === "video" && (
        <div className="absolute inset-0 grid place-items-center pointer-events-none">
          <span className="w-9 h-9 rounded-full bg-black/50 text-white grid place-items-center">
            <Play size={16} className="ml-0.5" />
          </span>
        </div>
      )}

      <div className="absolute inset-x-0 bottom-0 px-2 py-1 bg-gradient-to-t from-black/70 to-transparent text-white">
        <div className="text-xs font-semibold">{formatBytes(r.bytes)}</div>
        {label && <div className="text-[10px] truncate opacity-90">{label}</div>}
      </div>

      <button
        onClick={(e) => {
          e.stopPropagation();
          onToggle();
        }}
        title={selected ? "Unselect" : "Select"}
        className={cn(
          "absolute top-1.5 left-1.5 rounded bg-black/40 text-white p-0.5 transition",
          selected || selecting ? "opacity-100" : "opacity-0 group-hover:opacity-100",
        )}
      >
        {selected ? <CheckSquare size={16} /> : <Square size={16} />}
      </button>
      {!selecting && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            onDelete();
          }}
          title="Delete from this computer"
          className="absolute top-1.5 right-1.5 rounded bg-black/40 text-white p-1 opacity-0 group-hover:opacity-100 hover:bg-red-600 transition"
        >
          <Trash2 size={14} />
        </button>
      )}
    </div>
  );
}
