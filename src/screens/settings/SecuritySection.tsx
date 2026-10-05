import { useState } from "react";
import { Lock } from "lucide-react";
import { Button } from "@/components/ui";
import { confirm } from "@/components/Confirm";
import { SetPinDialog } from "@/components/SetPinDialog";
import { LOCK_IDLE_OPTIONS } from "@/lib/appLock";
import { usingFallback } from "@/lib/secrets";
import { useSettings } from "@/store/settings";
import { useAppLock } from "@/store/appLock";
import { Toggle } from "./shared";

export function SecuritySection() {
  const enabled = useSettings((s) => s.appLockEnabled);
  const idleMinutes = useSettings((s) => s.appLockIdleMinutes);
  const onHide = useSettings((s) => s.appLockOnHide);
  const onStart = useSettings((s) => s.appLockOnStart);
  const save = useSettings((s) => s.save);
  const pinSet = useAppLock((s) => s.pinSet);
  const lock = useAppLock((s) => s.lock);
  const clearPin = useAppLock((s) => s.clearPin);
  const [dialog, setDialog] = useState(false);

  const onToggle = (on: boolean) => {
    if (on) {
      // A PIN is required first; enabling is deferred until it has been set.
      if (pinSet) void save({ appLockEnabled: true });
      else setDialog(true);
    } else {
      void save({ appLockEnabled: false });
    }
  };

  return (
    <div className="space-y-4">
      <Toggle
        label="App lock"
        hint="Require a 4-digit PIN to open Wahana after it has been idle."
        checked={enabled && pinSet}
        onChange={onToggle}
      />
      {enabled && pinSet && (
        <>
          <label className="flex items-center gap-3 text-sm">
            <span className="flex-1">Auto-lock after</span>
            <select
              value={idleMinutes}
              onChange={(e) => void save({ appLockIdleMinutes: Number(e.target.value) })}
              className="rounded-lg border border-neutral-300 dark:border-neutral-700 bg-white dark:bg-neutral-900 px-2 py-1.5 text-sm outline-none"
            >
              {LOCK_IDLE_OPTIONS.map((m) => (
                <option key={m} value={m}>
                  {m === 1 ? "1 minute" : `${m} minutes`}
                </option>
              ))}
            </select>
          </label>
          <Toggle
            label="Lock when hidden to tray"
            hint="Lock when you close the window to the tray."
            checked={onHide}
            onChange={(v) => void save({ appLockOnHide: v })}
          />
          <Toggle
            label="Lock on app start"
            hint="Ask for the PIN every time Wahana opens."
            checked={onStart}
            onChange={(v) => void save({ appLockOnStart: v })}
          />
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="secondary" onClick={() => setDialog(true)}>
              Change PIN
            </Button>
            <Button variant="secondary" onClick={() => lock()}>
              <Lock size={14} /> Lock now
            </Button>
            <Button
              variant="ghost"
              onClick={async () => {
                if (
                  await confirm({
                    title: "Remove app lock PIN?",
                    message: "Wahana will open without a PIN.",
                    danger: true,
                    confirmLabel: "Remove PIN",
                  })
                ) {
                  await clearPin();
                  await save({ appLockEnabled: false });
                }
              }}
            >
              Remove PIN
            </Button>
          </div>
          {usingFallback() && (
            <p className="text-xs text-amber-600 dark:text-amber-400">
              The OS keychain is unavailable, so the PIN is kept in a local file (unencrypted).
            </p>
          )}
        </>
      )}
      {dialog && <SetPinDialog onClose={() => setDialog(false)} onDone={() => void save({ appLockEnabled: true })} />}
    </div>
  );
}
