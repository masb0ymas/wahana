import { useEffect, useRef, useState } from "react";
import { Delete } from "lucide-react";
import { cn } from "@/lib/utils";
import { useEvent } from "@/lib/hooks";

export const PIN_LENGTH = 4;

/** How long the completed PIN stays on screen before `onComplete` fires, so the last dot paints. */
const COMPLETE_DELAY_MS = 180;

/** 4-digit PIN entry: dots plus a numeric keypad, also driven by the physical keyboard. */
export function PinPad({
  value,
  onChange,
  onComplete,
  disabled,
  error,
}: {
  value: string;
  onChange: (v: string) => void;
  onComplete?: (pin: string) => void;
  disabled?: boolean;
  error?: boolean;
}) {
  const [busy, setBusy] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  const locked = disabled || busy;

  const enter = useEvent((digit: string) => {
    if (locked || value.length >= PIN_LENGTH) return;
    const next = value + digit;
    onChange(next);
    if (next.length === PIN_LENGTH) {
      // Hold the full row of dots briefly before handing off, otherwise the parent resets
      // `value` in the same render and the final dot never appears.
      setBusy(true);
      timer.current = setTimeout(() => {
        setBusy(false);
        onComplete?.(next);
      }, COMPLETE_DELAY_MS);
    }
  });

  const back = useEvent(() => {
    if (locked) return;
    onChange(value.slice(0, -1));
  });

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key >= "0" && e.key <= "9") {
        e.preventDefault();
        enter(e.key);
      } else if (e.key === "Backspace") {
        e.preventDefault();
        back();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [enter, back]);

  return (
    <div className="flex flex-col items-center gap-6">
      <div className="flex items-center justify-center gap-3">
        {Array.from({ length: PIN_LENGTH }).map((_, i) => (
          <span
            key={i}
            className={cn(
              "h-3.5 w-3.5 rounded-full border-2 transition",
              i < value.length
                ? error
                  ? "border-red-500 bg-red-500"
                  : "border-wa-dark bg-wa-dark"
                : "border-neutral-300 dark:border-neutral-600",
            )}
          />
        ))}
      </div>
      <div className="grid grid-cols-3 gap-2">
        {["1", "2", "3", "4", "5", "6", "7", "8", "9", "", "0", "back"].map((k, i) =>
          k === "" ? (
            <span key={i} />
          ) : (
            <button
              key={i}
              type="button"
              disabled={locked}
              onClick={() => (k === "back" ? back() : enter(k))}
              className="h-14 w-14 rounded-full text-lg font-medium grid place-items-center transition hover:bg-neutral-100 active:bg-neutral-200 disabled:opacity-40 disabled:cursor-not-allowed dark:hover:bg-neutral-800 dark:active:bg-neutral-700"
            >
              {k === "back" ? <Delete size={18} /> : k}
            </button>
          ),
        )}
      </div>
    </div>
  );
}
