import { BarChart3, ExternalLink, MapPin, MessageCircle, Phone, User } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { NativeInteractive } from "@/lib/nativeWa";
import { cn } from "@/lib/utils";

const card = "rounded-lg bg-black/5 dark:bg-white/10 px-3 py-2 min-w-[200px] max-w-[320px] text-left";

/** A poll as sent: its question and options, read-only (this client cannot vote). */
export function PollCard({ data }: { data: NonNullable<NativeInteractive["poll"]> }) {
  return (
    <div className={card}>
      <div className="flex items-center gap-1.5 text-[11px] font-medium text-neutral-500 dark:text-neutral-400">
        <BarChart3 size={13} /> Poll
      </div>
      <div className="mt-0.5 text-sm font-medium break-words">{data.question || "Poll"}</div>
      {data.options.length > 0 && (
        <ul className="mt-1 space-y-0.5">
          {data.options.map((option, i) => (
            <li key={i} className="flex items-center gap-2 text-sm">
              <span className="h-3.5 w-3.5 shrink-0 rounded-full border border-neutral-400 dark:border-neutral-500" />
              <span className="break-words">{option}</span>
            </li>
          ))}
        </ul>
      )}
      <div className="mt-1 text-[10px] text-neutral-500 dark:text-neutral-400">
        {data.multiple ? "Multiple answers allowed" : "Select one option"}
      </div>
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
