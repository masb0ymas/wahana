import { create } from "zustand";

/** AI reply suggestions shown under a bubble: message id → { items | error | loading }. In-memory only. */
export interface ReplySuggestion {
  items?: string[];
  error?: string;
  loading?: boolean;
  /** Re-runs the suggestion with the same chat context. */
  regen?: () => void;
}
interface State {
  byMsg: Record<string, ReplySuggestion>;
  set: (id: string, e: ReplySuggestion) => void;
  clear: (id: string) => void;
}
export const useReplySuggest = create<State>((set) => ({
  byMsg: {},
  set: (id, e) => set((st) => ({ byMsg: { ...st.byMsg, [id]: e } })),
  clear: (id) =>
    set((st) => {
      const byMsg = { ...st.byMsg };
      delete byMsg[id];
      return { byMsg };
    }),
}));
