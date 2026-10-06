import { create } from "zustand";
import { load, type Store } from "@tauri-apps/plugin-store";

/**
 * Blur display prefs for media. Everything starts blurred so a walk-by cannot read it, and a
 * "Reveal all" toggle is the only way out: `revealAll` for the Media screen, and `chatRevealAll`
 * (keyed `accountId:chatId`) for the photos and videos inside a chat. Both stay on until the
 * user blurs everything again.
 */
const STORE_FILE = "media-prefs.json";
let storePromise: Promise<Store> | null = null;
const store = () => (storePromise ??= load(STORE_FILE, { autoSave: true, defaults: {} }));
let flush: ReturnType<typeof setTimeout> | undefined;

interface State {
  /** Media-screen override: every tile shown unblurred, until "Blur all" turns it off. */
  revealAll: boolean;
  /** Chats whose media is shown unblurred, keyed `accountId:chatId`. */
  chatRevealAll: Record<string, 1>;
  hydrate: () => Promise<void>;
  setRevealAll: (on: boolean) => void;
  toggleChatReveal: (key: string) => void;
}

/** Settles once the stored prefs are loaded, so a save never writes the empty defaults over them. */
let hydrating: Promise<void> | null = null;

export const useMediaBlur = create<State>((set) => ({
  revealAll: false,
  chatRevealAll: {},
  hydrate() {
    hydrating = (async () => {
      const s = await store();
      set({
        revealAll: (await s.get<boolean>("revealAll")) ?? false,
        chatRevealAll: (await s.get<Record<string, 1>>("chatRevealAll")) ?? {},
      });
    })();
    return hydrating;
  },
  setRevealAll(on) {
    set({ revealAll: on });
    schedule();
  },
  toggleChatReveal(key) {
    set((st) => {
      const next = { ...st.chatRevealAll };
      if (next[key]) delete next[key];
      else next[key] = 1;
      return { chatRevealAll: next };
    });
    schedule();
  },
}));

function schedule() {
  clearTimeout(flush);
  flush = setTimeout(
    () =>
      void Promise.all([store(), hydrating]).then(async ([s]) => {
        const st = useMediaBlur.getState();
        await s.set("revealAll", st.revealAll);
        await s.set("chatRevealAll", st.chatRevealAll);
      }),
    500,
  );
}
