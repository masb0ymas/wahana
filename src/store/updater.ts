import { create } from "zustand";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { errMsg } from "@/lib/utils";

/** Outcome of the last update check, for the About screen's feedback. */
export type CheckResult = "idle" | "checking" | "available" | "latest" | "error";

export interface UpdaterState {
  update: Update | null;
  progress: number | null; // 0..1 while downloading
  error: string | null;
  status: CheckResult;
  checkNow: () => Promise<void>;
  install: () => Promise<void>;
  dismiss: () => void;
}

const DEV_NOTICE = "Update checks are disabled in development builds.";

/** Shared so the startup check, the banner and the manual About button stay in sync. */
export const useUpdaterStore = create<UpdaterState>((set, get) => ({
  update: null,
  progress: null,
  error: null,
  status: "idle",

  async checkNow() {
    if (import.meta.env.DEV) {
      set({ status: "error", error: DEV_NOTICE });
      return;
    }
    set({ status: "checking", error: null });
    try {
      const u = await check();
      if (u) set({ update: u, status: "available" });
      else set({ status: "latest" });
    } catch (e) {
      set({ status: "error", error: errMsg(e) });
    }
  },

  async install() {
    const update = get().update;
    if (!update) return;
    set({ error: null, progress: 0 });
    let total = 0;
    let got = 0;
    try {
      await update.downloadAndInstall((ev) => {
        if (ev.event === "Started") total = ev.data.contentLength ?? 0;
        else if (ev.event === "Progress") {
          got += ev.data.chunkLength;
          if (total) set({ progress: got / total });
        } else if (ev.event === "Finished") set({ progress: 1 });
      });
      await relaunch();
    } catch (e) {
      set({ error: errMsg(e), progress: null });
    }
  },

  dismiss: () => set({ update: null }),
}));
