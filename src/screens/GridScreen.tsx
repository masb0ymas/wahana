import { useEffect, useMemo, useRef, useState } from "react";
import { Minus, Plus } from "lucide-react";
import { AccountChatTile } from "@/components/AccountChatTile";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { Button } from "@/components/ui";
import { useSettings } from "@/store/settings";
import { useWhatsApp } from "@/store/whatsapp";
import type { NativeAccount } from "@/lib/nativeWa";

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
  const savedOrder = useSettings((s) => s.gridOrder);
  const [height, setHeight] = useState(0);
  const areaRef = useRef<HTMLDivElement>(null);
  const { ordered, dragging, handleFor, tileRef } = useTileOrder(accounts, savedOrder, areaRef);

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
          <Button size="sm" variant="secondary" onClick={() => void add().catch(console.error)}>
            <Plus size={14} /> Link account
          </Button>
        </div>
      </div>
      {/* The scrollbar lives in the page's right padding (stable gutter, pulled out by -mr-2.5 =
          8px track + 2px), so tiles keep the header's right edge whether or not the grid overflows. */}
      <div ref={areaRef} className="flex-1 min-h-0 overflow-y-auto -mr-2.5 pr-0.5 [scrollbar-gutter:stable]">
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
            {ordered.map((a) => (
              <div key={a.id} ref={tileRef(a.id)} className="min-h-0">
                <ErrorBoundary inline label="account">
                  <AccountChatTile account={a} dragging={dragging === a.id} handle={handleFor(a.id)} />
                </ErrorBoundary>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

/** Saved order first (ids of unlinked accounts dropped), then accounts not placed yet, as listed. */
function arrange(accounts: Pick<NativeAccount, "id">[], order: string[]) {
  const byId = new Map(accounts.map((a) => [a.id, a]));
  const placed = order.filter((id) => byId.has(id));
  const rest = accounts.filter((a) => !placed.includes(a.id)).map((a) => a.id);
  return [...placed, ...rest];
}

/**
 * Reorders tiles by dragging their header. Pointer events, not HTML5 drag-and-drop, which
 * Tauri's window drag-drop handler breaks on Windows (same as the chat filter tabs). The
 * tile nearest the pointer by its center takes the dragged one's slot as it moves.
 */
function useTileOrder(accounts: NativeAccount[], savedOrder: string[], areaRef: React.RefObject<HTMLDivElement | null>) {
  const [order, setOrder] = useState(() => arrange(accounts, savedOrder));
  const orderRef = useRef(order);
  const drag = useRef<{ id: string; x: number; y: number; active: boolean } | null>(null);
  orderRef.current = order;
  // Accounts are re-sent on every unread change; only a linked or removed account (or a saved
  // order) re-arranges, and never in the middle of a drag.
  const ids = accounts.map((a) => a.id).join(",");
  useEffect(() => {
    if (!drag.current?.active) setOrder(arrange(ids ? ids.split(",").map((id) => ({ id })) : [], savedOrder));
  }, [ids, savedOrder]);
  const ordered = useMemo(() => {
    const byId = new Map(accounts.map((a) => [a.id, a]));
    return order.map((id) => byId.get(id)).filter((a): a is NativeAccount => !!a);
  }, [accounts, order]);

  const tiles = useRef(new Map<string, HTMLDivElement>());
  const tileRef = (id: string) => (el: HTMLDivElement | null) => {
    if (el) tiles.current.set(id, el);
    else tiles.current.delete(id);
  };
  const [dragging, setDragging] = useState<string | null>(null);

  const handleFor = (id: string): React.HTMLAttributes<HTMLDivElement> | undefined =>
    accounts.length < 2
      ? undefined
      : {
          onPointerDown: (e) => {
            if (e.button !== 0) return;
            e.currentTarget.setPointerCapture(e.pointerId);
            drag.current = { id, x: e.clientX, y: e.clientY, active: false };
          },
          onPointerMove: (e) => {
            const d = drag.current;
            if (!d) return;
            if (!d.active) {
              // A small threshold keeps a plain click from starting a drag.
              if (Math.abs(e.clientX - d.x) < 4 && Math.abs(e.clientY - d.y) < 4) return;
              d.active = true;
              setDragging(d.id);
            }
            // Dragging near the top or bottom edge scrolls the grid, so off-screen rows can be reached.
            const area = areaRef.current;
            if (area) {
              const r = area.getBoundingClientRect();
              if (e.clientY < r.top + 40) area.scrollTop -= 12;
              else if (e.clientY > r.bottom - 40) area.scrollTop += 12;
            }
            let nearest = d.id;
            let best = Infinity;
            for (const [tid, el] of tiles.current) {
              const r = el.getBoundingClientRect();
              const dist = Math.hypot(e.clientX - (r.left + r.width / 2), e.clientY - (r.top + r.height / 2));
              if (dist < best) {
                best = dist;
                nearest = tid;
              }
            }
            if (nearest === d.id) return;
            setOrder((o) => {
              const from = o.indexOf(d.id);
              const to = o.indexOf(nearest);
              if (from < 0 || to < 0) return o;
              const next = [...o];
              next.splice(from, 1);
              next.splice(to, 0, d.id);
              return next;
            });
          },
          onPointerUp: () => {
            const d = drag.current;
            drag.current = null;
            if (!d?.active) return;
            setDragging(null);
            void useSettings.getState().save({ gridOrder: orderRef.current });
          },
          onPointerCancel: () => {
            drag.current = null;
            setDragging(null);
            setOrder(arrange(accounts, useSettings.getState().gridOrder));
          },
        };

  return { ordered, dragging, handleFor, tileRef };
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
