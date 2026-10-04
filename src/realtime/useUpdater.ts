import { useEffect } from "react";
import { useUpdaterStore, type UpdaterState } from "@/store/updater";

export type { UpdaterState } from "@/store/updater";

/** Checks GitHub Releases on startup (and every 6h) for a signed update. */
export function useUpdater(): UpdaterState {
  const state = useUpdaterStore();
  useEffect(() => {
    if (import.meta.env.DEV) return; // dev builds have no matching release
    const run = () => void useUpdaterStore.getState().checkNow();
    run();
    const timer = setInterval(run, 6 * 60 * 60 * 1000);
    return () => clearInterval(timer);
  }, []);
  return state;
}
