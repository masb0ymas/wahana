import { create } from "zustand";
import { load, type Store } from "@tauri-apps/plugin-store";

/**
 * Per-account visual identity: a colour and an emoji the user picks, so several linked sessions
 * can be told apart in the sidebar, account picker, grid and notifications. Keyed by account id.
 */
const STORE_FILE = "account-styles.json";
let storePromise: Promise<Store> | null = null;
const store = () => (storePromise ??= load(STORE_FILE, { autoSave: true, defaults: {} }));
let flush: ReturnType<typeof setTimeout> | undefined;

/** Palette offered in the account card; readable on light and dark backgrounds. */
export const ACCOUNT_COLORS = [
  "#25d366",
  "#0ea5e9",
  "#8b5cf6",
  "#f59e0b",
  "#ef4444",
  "#ec4899",
  "#14b8a6",
  "#6366f1",
  "#84cc16",
  "#f97316",
] as const;

export interface AccountStyle {
  /** Chosen accent colour; unset falls back to `defaultAccountColor(id)`. */
  color?: string;
  /** Chosen emoji marker; unset means no icon. */
  icon?: string;
}

/** Resolved identity for an account. */
export interface ResolvedStyle {
  color: string;
  icon?: string;
}

interface State {
  styles: Record<string, AccountStyle>;
  hydrate: () => Promise<void>;
  setColor: (id: string, color: string | null) => void;
  setIcon: (id: string, icon: string | null) => void;
  clear: (id: string) => void;
}

/** Stable auto colour so two accounts differ even before the user picks one. */
export function defaultAccountColor(id: string): string {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0;
  return ACCOUNT_COLORS[h % ACCOUNT_COLORS.length]!;
}

/** Resolve one account's identity, falling back to the auto colour. */
export function resolveStyle(styles: Record<string, AccountStyle>, id: string): ResolvedStyle {
  const s = styles[id];
  return { color: s?.color ?? defaultAccountColor(id), icon: s?.icon };
}

/** Patch one account's style, dropping the entry when it has no fields left. */
function patch(styles: Record<string, AccountStyle>, id: string, next: AccountStyle): Record<string, AccountStyle> {
  const merged: AccountStyle = { ...styles[id], ...next };
  if (merged.color === undefined) delete merged.color;
  if (merged.icon === undefined) delete merged.icon;
  const out = { ...styles };
  if (merged.color || merged.icon) out[id] = merged;
  else delete out[id];
  return out;
}

export const useAccountStyle = create<State>((set) => ({
  styles: {},
  async hydrate() {
    const s = await store();
    set({ styles: (await s.get<Record<string, AccountStyle>>("styles")) ?? {} });
  },
  setColor(id, color) {
    set((st) => ({ styles: patch(st.styles, id, { color: color ?? undefined }) }));
    schedule();
  },
  setIcon(id, icon) {
    set((st) => ({ styles: patch(st.styles, id, { icon: icon ?? undefined }) }));
    schedule();
  },
  clear(id) {
    set((st) => {
      if (!(id in st.styles)) return st;
      const { [id]: _, ...rest } = st.styles;
      return { styles: rest };
    });
    schedule();
  },
}));

/** Reactive identity for a single account. */
export function useAccountStyleFor(id: string): ResolvedStyle {
  const style = useAccountStyle((s) => s.styles[id]);
  return { color: style?.color ?? defaultAccountColor(id), icon: style?.icon };
}

function schedule() {
  clearTimeout(flush);
  flush = setTimeout(() => void store().then((s) => s.set("styles", useAccountStyle.getState().styles)), 500);
}
