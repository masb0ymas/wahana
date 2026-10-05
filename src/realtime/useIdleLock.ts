import { useEffect, useRef } from "react";
import { useSettings } from "@/store/settings";
import { useAppLock } from "@/store/appLock";

const CHECK_INTERVAL_MS = 5_000;
const ACTIVITY_EVENTS = ["mousemove", "mousedown", "wheel", "keydown", "touchstart"] as const;

/** Auto-locks the app after a configurable idle period. Mounted once at the app root. */
export function useIdleLock() {
  const hydrated = useSettings((s) => s.hydrated);
  const enabled = useSettings((s) => s.appLockEnabled);
  const idleMinutes = useSettings((s) => s.appLockIdleMinutes);
  const pinSet = useAppLock((s) => s.pinSet);
  const locked = useAppLock((s) => s.locked);
  const lastActivity = useRef(Date.now());

  useEffect(() => {
    if (!hydrated || !enabled || !pinSet || locked) return;
    const mark = () => {
      lastActivity.current = Date.now();
    };
    const check = () => {
      if (Date.now() - lastActivity.current >= idleMinutes * 60_000) useAppLock.getState().lock();
    };
    // Start the clock now so enabling the lock never triggers immediately.
    mark();
    for (const e of ACTIVITY_EVENTS) window.addEventListener(e, mark, { passive: true });
    // On waking (refocus / tab visible again) a long absence should lock, so check instead of marking.
    window.addEventListener("focus", check);
    document.addEventListener("visibilitychange", check);
    const timer = setInterval(check, CHECK_INTERVAL_MS);
    return () => {
      for (const e of ACTIVITY_EVENTS) window.removeEventListener(e, mark);
      window.removeEventListener("focus", check);
      document.removeEventListener("visibilitychange", check);
      clearInterval(timer);
    };
  }, [hydrated, enabled, pinSet, locked, idleMinutes]);
}
