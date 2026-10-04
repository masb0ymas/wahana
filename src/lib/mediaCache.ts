import { invoke } from "@tauri-apps/api/core";
import { useSettings } from "@/store/settings";

export interface CacheStats {
  bytes: number;
  files: number;
  path: string;
}

/** Cache key for a message's media: message id + extension. */
export function mediaCacheKey(messageId: string, mimetype: string) {
  const ext = (mimetype.split("/")[1] ?? "bin").split(";")[0]!.replace("jpeg", "jpg");
  return `${messageId}.${ext}`;
}

export async function cacheGet(key: string): Promise<Blob | null> {
  try {
    if (!(await invoke<boolean>("media_cache_has", { key }))) return null;
    const buf = await invoke<ArrayBuffer>("media_cache_get", { key });
    return new Blob([buf]);
  } catch {
    return null;
  }
}

/** What the Media screen knows about a cached file, saved beside it. */
export interface CacheMeta {
  account: string;
  /** Kept so the account's media can still be named after the account is removed. */
  accountName?: string;
  chatId: string;
  messageId: string;
  mimetype: string;
  kind: string;
  fileName: string | null;
  /** The message's time (unix ms). */
  timestamp: number;
  fromMe: boolean;
  sender: string;
}

export interface CacheEntry {
  file: string;
  bytes: number;
  /** When it was cached (unix ms). */
  saved: number;
  /** Missing for files cached before metadata was kept. */
  meta: CacheMeta | null;
}

// Header values must be ASCII: escape the rest as JSON `\uXXXX`, which still parses.
const asciiJson = (v: unknown) =>
  JSON.stringify(v).replace(/[\u0080-\uffff]/g, (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, "0")}`);

export async function cachePut(key: string, blob: Blob, meta?: CacheMeta) {
  const limitMb = useSettings.getState().cacheLimitMb;
  const bytes = new Uint8Array(await blob.arrayBuffer());
  const headers: Record<string, string> = { "x-key": key, "x-limit": String(Math.max(0, limitMb) * 1024 * 1024) };
  if (meta) headers["x-meta"] = asciiJson(meta);
  await invoke("media_cache_put", bytes, { headers }).catch((e) => console.warn("cache put failed", e));
}

export const cacheStats = () => invoke<CacheStats>("media_cache_stats");
export const cacheClear = () => invoke<void>("media_cache_clear");
export const cacheList = () => invoke<CacheEntry[]>("media_cache_list");
export const cacheRead = (file: string) => invoke<ArrayBuffer>("media_cache_read", { file });
export const cacheDelete = (files: string[]) => invoke<number>("media_cache_delete", { files });

export function formatBytes(b: number) {
  if (b < 1024) return `${b} B`;
  if (b < 1024 ** 2) return `${(b / 1024).toFixed(0)} KB`;
  if (b < 1024 ** 3) return `${(b / 1024 ** 2).toFixed(1)} MB`;
  return `${(b / 1024 ** 3).toFixed(2)} GB`;
}
