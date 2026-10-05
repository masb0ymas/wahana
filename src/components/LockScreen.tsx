import { useEffect, useState } from "react";
import { Lock, ShieldAlert } from "lucide-react";
import { Button, Input } from "@/components/ui";
import { PinPad } from "@/components/PinPad";
import { useAppLock } from "@/store/appLock";
import { cn } from "@/lib/utils";

/** Full-screen lock overlay; rendered at the app root while `locked`. */
export function LockScreen() {
  const locked = useAppLock((s) => s.locked);
  const verify = useAppLock((s) => s.verify);
  const clearPin = useAppLock((s) => s.clearPin);
  const lockoutUntil = useAppLock((s) => s.lockoutUntil);

  const [value, setValue] = useState("");
  const [error, setError] = useState(false);
  const [nonce, setNonce] = useState(0);
  const [now, setNow] = useState(() => Date.now());
  const [resetting, setResetting] = useState(false);
  const [resetText, setResetText] = useState("");

  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);

  if (!locked) return null;

  const remaining = Math.max(0, Math.ceil((lockoutUntil - now) / 1000));
  const blocked = remaining > 0;

  const submit = async (pin: string) => {
    if (blocked) {
      setValue("");
      return;
    }
    if (!(await verify(pin))) {
      setValue("");
      setError(true);
      setNonce((n) => n + 1);
    }
  };

  return (
    <div className="fixed inset-0 z-[100] bg-black/70 backdrop-blur-sm grid place-items-center">
      <div key={nonce} className={cn("w-[340px] rounded-2xl bg-white dark:bg-neutral-900 shadow-2xl p-6", error && "shake")}>
        <div className="flex flex-col items-center gap-1 mb-5">
          <div className="h-12 w-12 rounded-full bg-wa-dark text-white grid place-items-center mb-1">
            <Lock size={22} />
          </div>
          <h1 className="font-semibold">Wahana is locked</h1>
          <p className="text-xs text-neutral-500">Enter your 4-digit PIN to continue</p>
        </div>
        <PinPad
          value={value}
          onChange={(v) => {
            setValue(v);
            setError(false);
          }}
          onComplete={submit}
          disabled={blocked || resetting}
          error={error}
        />
        {blocked ? (
          <p className="mt-4 text-center text-xs text-amber-600 dark:text-amber-400">Too many attempts. Try again in {remaining}s.</p>
        ) : (
          error && <p className="mt-4 text-center text-xs text-red-600">Incorrect PIN. Try again.</p>
        )}
        <div className="mt-4 text-center">
          <button type="button" className="text-xs text-neutral-500 hover:text-wa-dark" onClick={() => setResetting(true)}>
            Forgot PIN?
          </button>
        </div>
      </div>
      {resetting && (
        <div className="absolute inset-0 z-10 grid place-items-center bg-black/60 p-6">
          <div className="w-[380px] rounded-2xl bg-white dark:bg-neutral-900 shadow-2xl p-5">
            <div className="flex items-center gap-2 mb-2 text-amber-600">
              <ShieldAlert size={18} />
              <h2 className="font-semibold">Reset app lock?</h2>
            </div>
            <p className="text-sm text-neutral-600 dark:text-neutral-300 mb-3">
              This removes the PIN and unlocks Wahana. Your messages and data are not deleted. Type <b>RESET</b> to confirm.
            </p>
            <Input value={resetText} onChange={(e) => setResetText(e.target.value)} placeholder="Type RESET" autoFocus />
            <div className="mt-4 flex justify-end gap-2">
              <Button
                variant="ghost"
                onClick={() => {
                  setResetting(false);
                  setResetText("");
                }}
              >
                Cancel
              </Button>
              <Button variant="danger" disabled={resetText !== "RESET"} onClick={() => void clearPin()}>
                Reset & unlock
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
