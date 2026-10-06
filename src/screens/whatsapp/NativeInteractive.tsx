import { useEffect, useState } from "react";
import { BarChart3, Check, ExternalLink, MapPin, MessageCircle, Phone, User } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { NativeInteractive } from "@/lib/nativeWa";
import { cn, errMsg } from "@/lib/utils";

const card = "rounded-lg bg-black/5 dark:bg-white/10 px-3 py-2 min-w-[200px] max-w-[320px] text-left";

/** A poll with its votes so far. With `onVote` (the poll's key is stored and the account is
 * connected) the options are clickable: a single-choice poll takes one pick (clicking it again
 * withdraws it), a multiple-choice one toggles each. */
export function PollCard({
  data,
  onVote,
}: {
  data: NonNullable<NativeInteractive["poll"]>;
  onVote?: (options: string[]) => Promise<void>;
}) {
  const results = data.results;
  const mine = results?.mine ?? [];
  // The pick just sent, shown until the stored vote comes back with it.
  const [pending, setPending] = useState<string[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [showVoters, setShowVoters] = useState(false);
  const mineKey = mine.join("\u0000");
  useEffect(() => setPending(null), [mineKey]);
  const picked = pending ?? mine;
  const counts = data.options.map((o, i) => (results?.voters[i]?.length ?? 0) + (picked.includes(o) ? 1 : 0));
  const voters = new Set(results?.voters.flat() ?? []).size + (picked.length > 0 ? 1 : 0);
  const anyVotes = counts.some((c) => c > 0);

  const toggle = (option: string) => {
    if (!onVote) return;
    const next = data.multiple
      ? picked.includes(option)
        ? picked.filter((o) => o !== option)
        : data.options.filter((o) => o === option || picked.includes(o))
      : picked.includes(option)
        ? []
        : [option];
    setPending(next);
    setErr(null);
    onVote(next).catch((e) => {
      setPending(null);
      setErr(errMsg(e));
    });
  };

  return (
    <div className={card}>
      <div className="flex items-center gap-1.5 text-[11px] font-medium text-neutral-600 dark:text-white/70">
        <BarChart3 size={13} /> Poll
      </div>
      <div className="mt-0.5 text-sm font-medium break-words">{data.question || "Poll"}</div>
      <div className="mt-0.5 text-[10px] text-neutral-600 dark:text-white/70">{data.multiple ? "Select one or more" : "Select one"}</div>
      {data.options.length > 0 && (
        <ul className="mt-1.5 space-y-1.5">
          {data.options.map((option, i) => {
            const on = picked.includes(option);
            const share = voters > 0 ? counts[i]! / voters : 0;
            return (
              <li key={i}>
                <button
                  type="button"
                  disabled={!onVote}
                  onClick={() => toggle(option)}
                  aria-pressed={on}
                  className="group w-full text-left disabled:cursor-default"
                >
                  <span className="flex items-center gap-2 text-sm">
                    <span
                      className={cn(
                        "grid h-4 w-4 shrink-0 place-items-center border",
                        data.multiple ? "rounded" : "rounded-full",
                        on
                          ? "border-wa-teal bg-wa-teal text-white dark:border-white dark:bg-white dark:text-wa-teal"
                          : "border-neutral-500 dark:border-white/60 group-enabled:group-hover:border-wa-teal dark:group-enabled:group-hover:border-white",
                      )}
                    >
                      {on && <Check size={11} strokeWidth={3} />}
                    </span>
                    <span className="min-w-0 flex-1 break-words">{option}</span>
                    {anyVotes && <span className="text-xs tabular-nums text-neutral-600 dark:text-white/70">{counts[i]}</span>}
                  </span>
                  {anyVotes && (
                    <span className="mt-1 ml-6 block h-1 rounded-full bg-black/10 dark:bg-white/15">
                      <span
                        className="block h-full rounded-full bg-wa-teal dark:bg-white transition-[width]"
                        style={{ width: `${Math.round(share * 100)}%` }}
                      />
                    </span>
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      )}
      {err && <div className="mt-1 text-[11px] text-red-600 dark:text-red-400 break-words">{err}</div>}
      {results && !results.canVote && (
        <div className="mt-1.5 text-[10px] text-neutral-600 dark:text-white/70">
          This poll arrived before votes were supported here; vote from your phone.
        </div>
      )}
      {anyVotes && (
        <div className="mt-1.5 border-t border-black/10 dark:border-white/10 pt-1">
          <button
            type="button"
            onClick={() => setShowVoters((v) => !v)}
            className="text-[11px] font-medium text-wa-teal dark:text-white underline-offset-2 hover:underline"
          >
            {showVoters ? "Hide votes" : `View votes (${voters})`}
          </button>
          {showVoters && (
            <ul className="mt-1 space-y-1 text-[11px]">
              {data.options.map((option, i) => {
                const names = [...(picked.includes(option) ? ["You"] : []), ...(results?.voters[i] ?? [])];
                if (names.length === 0) return null;
                return (
                  <li key={i}>
                    <div className="font-medium break-words">{option}</div>
                    <div className="text-neutral-600 dark:text-white/70 break-words selectable">{names.join(", ")}</div>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}

/** A shared location: its name/address and coordinates, opening in Maps. */
export function LocationCard({ data }: { data: NonNullable<NativeInteractive["location"]> }) {
  const title = data.name || data.address || "Location";
  const maps = `https://www.google.com/maps/search/?api=1&query=${data.latitude},${data.longitude}`;
  return (
    <button onClick={() => void openUrl(maps)} title="Open in Maps" className={cn(card, "hover:bg-black/10 dark:hover:bg-white/15")}>
      <div className="flex items-start gap-2">
        <MapPin size={26} className="mt-0.5 shrink-0 text-neutral-500" />
        <div className="min-w-0 flex-1">
          <div className="text-sm font-medium break-words">{title}</div>
          {data.address && data.name && <div className="text-[11px] text-neutral-500 break-words">{data.address}</div>}
          <div className="text-[11px] text-neutral-500 selectable">
            {data.latitude}, {data.longitude}
          </div>
          <div className="mt-0.5 flex items-center gap-1 text-[10px] text-wa-dark dark:text-wa">
            {data.live && <span className="mr-1 rounded bg-black/5 dark:bg-white/10 px-1">Live location</span>}
            <ExternalLink size={11} /> Open in Maps
          </div>
        </div>
      </div>
    </button>
  );
}

/** One or more shared contacts; one with a number opens its chat here when clicked. */
export function ContactCard({ data, onOpen }: { data: NonNullable<NativeInteractive["contacts"]>; onOpen: (phone: string) => void }) {
  return (
    <div className={card}>
      <div className="flex items-center gap-1.5 text-[11px] font-medium text-neutral-500 dark:text-neutral-400">
        <User size={13} /> {data.length > 1 ? `${data.length} contacts` : "Contact"}
      </div>
      <div className="mt-1 space-y-1.5">
        {data.map((contact, i) => (
          <button
            key={i}
            type="button"
            disabled={!contact.phone}
            onClick={() => contact.phone && onOpen(contact.phone)}
            title={contact.phone ? "Start chat" : undefined}
            className="group flex w-full items-center gap-2 rounded-md text-left enabled:hover:bg-black/5 enabled:dark:hover:bg-white/10 disabled:cursor-default"
          >
            <span className="h-8 w-8 shrink-0 grid place-items-center rounded-full bg-black/10 dark:bg-white/15 text-xs font-semibold">
              {contact.name.trim().charAt(0).toUpperCase() || "?"}
            </span>
            <span className="min-w-0 flex-1">
              <span className="block text-sm font-medium truncate group-enabled:group-hover:text-wa-dark group-enabled:dark:group-hover:text-wa">
                {contact.name}
              </span>
              {contact.phone && (
                <span className="flex items-center gap-1 text-[11px] text-neutral-500">
                  <Phone size={11} /> {contact.phone}
                </span>
              )}
            </span>
            {contact.phone && (
              <MessageCircle size={14} className="shrink-0 text-neutral-400 group-hover:text-wa-dark dark:group-hover:text-wa" />
            )}
          </button>
        ))}
      </div>
    </div>
  );
}
