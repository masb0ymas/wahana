import { useEffect, useRef, useState } from "react";
import { Minus, Plus } from "lucide-react";
import { AccountChatTile } from "@/components/AccountChatTile";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { useSettings } from "@/store/settings";
import { useWhatsApp } from "@/store/whatsapp";

const GAP = 12;
const MIN = 1;
const MAX_COLS = 6;
const MAX_ROWS = 6;
const MIN_ROW_HEIGHT = 140;

/** Several accounts side by side, each tile showing that account's chat list. */
export function GridScreen() {
  const accounts = useWhatsApp((s) => s.accounts);
  const add = useWhatsApp((s) => s.add);
  const columns = useSettings((s) => s.gridColumns);
  const rows = useSettings((s) => s.gridRows);
  const [height, setHeight] = useState(0);
  const areaRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = areaRef.current;
    if (!el) return;
    setHeight(el.clientHeight);
    const observer = new ResizeObserver((entries) => {
      const h = entries[0]?.contentRect.height;
      if (h !== undefined) setHeight(h);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // Fill exactly `rows` rows on screen; extra accounts land on more rows and scroll.
  const rowHeight = height > 0 ? Math.max(MIN_ROW_HEIGHT, (height - (rows - 1) * GAP) / rows) : 240;

  return (
    <div className="flex-1 min-w-0 flex flex-col p-4 gap-3">
      <div className="flex items-center gap-3 flex-wrap">
        <h1 className="text-xl font-semibold">Multi-account</h1>
        <div className="ml-auto flex items-center gap-4">
          <Stepper label="Columns" value={columns} max={MAX_COLS} onChange={(v) => void useSettings.getState().save({ gridColumns: v })} />
          <Stepper label="Rows" value={rows} max={MAX_ROWS} onChange={(v) => void useSettings.getState().save({ gridRows: v })} />
        </div>
      </div>
      <div ref={areaRef} className="flex-1 min-h-0 overflow-y-auto">
        {accounts.length === 0 ? (
          <div className="h-full grid place-items-center text-sm text-neutral-500">No WhatsApp account linked yet.</div>
        ) : (
          <div
            style={{
              display: "grid",
              gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
              gridAutoRows: `${rowHeight}px`,
              gap: GAP,
            }}
          >
            {accounts.map((a) => (
              <ErrorBoundary key={a.id} inline label="account">
                <AccountChatTile account={a} />
              </ErrorBoundary>
            ))}
            <button
              type="button"
              onClick={() => void add().catch(console.error)}
              className="flex flex-col items-center justify-center gap-1 rounded-xl border border-dashed border-neutral-300 dark:border-neutral-700 text-sm text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-800/60"
            >
              <Plus size={20} />
              Link a WhatsApp account
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function Stepper({ label, value, max, onChange }: { label: string; value: number; max: number; onChange: (value: number) => void }) {
  return (
    <div className="flex items-center gap-2 text-xs text-neutral-500">
      <span>{label}</span>
      <div className="flex items-center gap-1 rounded-lg bg-neutral-100 dark:bg-neutral-800 px-1 py-0.5">
        <button
          type="button"
          disabled={value <= MIN}
          onClick={() => onChange(value - 1)}
          className="p-1 rounded disabled:opacity-30 hover:text-neutral-800 dark:hover:text-neutral-200"
          title={`Fewer ${label.toLowerCase()}`}
        >
          <Minus size={12} />
        </button>
        <span className="w-4 text-center tabular-nums text-neutral-700 dark:text-neutral-200">{value}</span>
        <button
          type="button"
          disabled={value >= max}
          onClick={() => onChange(value + 1)}
          className="p-1 rounded disabled:opacity-30 hover:text-neutral-800 dark:hover:text-neutral-200"
          title={`More ${label.toLowerCase()}`}
        >
          <Plus size={12} />
        </button>
      </div>
    </div>
  );
}
