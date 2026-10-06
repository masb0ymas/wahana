import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { FileText, Search } from "lucide-react";
import { useDismiss } from "@/lib/hooks";
import { cn } from "@/lib/utils";
import { fillTemplate, listTemplates, markTemplateUsed, type Template } from "@/store/templates";

/**
 * Template list opened from the composer's "+" menu: searchable, most used first, each one
 * previewed with this chat's name and number already filled in. Picking one inserts it.
 */
export function TemplatePicker({
  account,
  ctx,
  onPick,
  onClose,
}: {
  /** Account key (`native:<id>`); only its templates are offered. */
  account: string;
  ctx: { name?: string | null; phone?: string | null };
  onPick: (text: string) => void;
  onClose: () => void;
}) {
  const qc = useQueryClient();
  const { data } = useQuery({ queryKey: ["templates", account], queryFn: () => listTemplates(account) });
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const ref = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  useDismiss(ref, true, onClose);

  const list = useMemo(() => {
    const t = query.trim().toLowerCase();
    const all = data ?? [];
    return t ? all.filter((x) => x.name.toLowerCase().includes(t) || x.text.toLowerCase().includes(t)) : all;
  }, [data, query]);
  useEffect(() => setIndex(0), [query]);
  useEffect(() => {
    listRef.current?.querySelector(`[data-index="${index}"]`)?.scrollIntoView({ block: "nearest" });
  }, [index]);

  const pick = (t: Template) => {
    void markTemplateUsed(t.id).then(() => qc.invalidateQueries({ queryKey: ["templates"] }));
    onPick(fillTemplate(t.text, ctx));
  };

  return (
    <div
      ref={ref}
      className="absolute bottom-full left-0 z-30 mb-2 w-96 max-w-[calc(100%-1rem)] overflow-hidden rounded-xl border border-neutral-200 bg-white shadow-xl dark:border-neutral-700 dark:bg-neutral-900"
    >
      <div className="flex items-center gap-2 border-b border-neutral-200 dark:border-neutral-800 px-3 py-2">
        <Search size={14} className="shrink-0 text-neutral-400" />
        <input
          autoFocus
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown" && list.length) {
              e.preventDefault();
              setIndex((i) => (i + 1) % list.length);
            } else if (e.key === "ArrowUp" && list.length) {
              e.preventDefault();
              setIndex((i) => (i - 1 + list.length) % list.length);
            } else if (e.key === "Enter" && list[index]) {
              e.preventDefault();
              pick(list[index]);
            } else if (e.key === "Escape") {
              e.preventDefault();
              onClose();
            }
          }}
          placeholder="Search templates"
          className="min-w-0 flex-1 bg-transparent text-sm outline-none"
        />
        <span className="shrink-0 text-[10px] text-neutral-400">↑↓ Enter</span>
      </div>
      <div ref={listRef} className="max-h-80 overflow-y-auto py-1">
        {list.map((t, i) => (
          <button
            key={t.id}
            data-index={i}
            onMouseEnter={() => setIndex(i)}
            onClick={() => pick(t)}
            className={cn("flex w-full items-start gap-2.5 px-3 py-2 text-left", i === index && "bg-neutral-100 dark:bg-neutral-800")}
          >
            <FileText size={15} className="mt-0.5 shrink-0 text-wa-dark dark:text-wa" />
            <span className="min-w-0">
              <span className="block text-sm font-medium">{t.name}</span>
              <span className="block text-xs text-neutral-600 dark:text-neutral-300 whitespace-pre-wrap line-clamp-3">
                {fillTemplate(t.text, ctx)}
              </span>
            </span>
          </button>
        ))}
        {list.length === 0 && (
          <div className="px-4 py-6 text-center text-xs text-neutral-500">
            {data?.length ? "No template matches." : "No templates yet. Add them in Features → Templates."}
          </div>
        )}
      </div>
    </div>
  );
}
