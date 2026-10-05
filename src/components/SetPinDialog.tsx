import { useState } from "react";
import { KeyRound } from "lucide-react";
import { Button } from "@/components/ui";
import { PinPad } from "@/components/PinPad";
import { useAppLock } from "@/store/appLock";

/** Two-phase (enter + confirm) modal for setting or changing the app-lock PIN. */
export function SetPinDialog({ onClose, onDone }: { onClose: () => void; onDone?: () => void }) {
  const setPin = useAppLock((s) => s.setPin);
  const [phase, setPhase] = useState<"enter" | "confirm">("enter");
  const [first, setFirst] = useState("");
  const [value, setValue] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [nonce, setNonce] = useState(0);

  const complete = async (pin: string) => {
    if (phase === "enter") {
      setFirst(pin);
      setValue("");
      setError("");
      setPhase("confirm");
      return;
    }
    if (pin !== first) {
      setFirst("");
      setValue("");
      setPhase("enter");
      setError("PINs don't match. Try again.");
      setNonce((n) => n + 1);
      return;
    }
    setBusy(true);
    try {
      await setPin(pin);
      onDone?.();
      onClose();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 bg-black/40 grid place-items-center" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="w-[340px] rounded-2xl bg-white dark:bg-neutral-900 shadow-2xl p-6">
        <div className="flex items-center gap-2 mb-1">
          <KeyRound size={18} className="text-wa-dark" />
          <h2 className="font-semibold">{phase === "enter" ? "Choose a PIN" : "Confirm PIN"}</h2>
        </div>
        <p className="text-xs text-neutral-500 mb-5">
          {phase === "enter" ? "Enter 4 digits. You'll need this to unlock Wahana." : "Enter the same 4 digits again."}
        </p>
        <div key={nonce} className={error ? "shake" : undefined}>
          <PinPad value={value} onChange={setValue} onComplete={complete} disabled={busy} error={!!error} />
        </div>
        {error && <p className="mt-4 text-center text-xs text-red-600">{error}</p>}
        <div className="mt-4 flex justify-end">
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
        </div>
      </div>
    </div>
  );
}
