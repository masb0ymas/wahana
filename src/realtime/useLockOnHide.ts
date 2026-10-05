import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSettings } from "@/store/settings";
import { useAppLock } from "@/store/appLock";

/** Locks the app when the window is closed to the tray (the Rust side hides it instead of quitting). */
export function useLockOnHide() {
  const hydrated = useSettings((s) => s.hydrated);
  const enabled = useSettings((s) => s.appLockEnabled);
  const onHide = useSettings((s) => s.appLockOnHide);
  const pinSet = useAppLock((s) => s.pinSet);

  useEffect(() => {
    if (!hydrated || !enabled || !onHide || !pinSet) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWindow()
      .onCloseRequested(() => {
        useAppLock.getState().lock();
      })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch((e) => console.warn("close-requested listener failed", e));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [hydrated, enabled, onHide, pinSet]);
}
