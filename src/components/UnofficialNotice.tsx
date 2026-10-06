import { TriangleAlert } from "lucide-react";
import { cn } from "@/lib/utils";

/**
 * The risk of using Wahana, stated plainly: it is not WhatsApp's own app, and WhatsApp can
 * ban a number that uses it. Shown on the welcome screen before linking, and in Settings → About.
 */
export function UnofficialNotice({ className }: { className?: string }) {
  return (
    <div
      role="note"
      className={cn(
        "flex gap-3 rounded-xl border border-amber-300 bg-amber-50 p-4 text-left text-sm text-amber-950",
        "dark:border-amber-500/40 dark:bg-amber-500/10 dark:text-amber-100",
        className,
      )}
    >
      <TriangleAlert size={20} className="mt-0.5 shrink-0 text-amber-600 dark:text-amber-400" />
      <div className="space-y-1.5">
        <p className="font-semibold">Unofficial app: use it at your own risk</p>
        <p>
          Wahana is not made by, affiliated with, or endorsed by WhatsApp or Meta. Using an unofficial client can break WhatsApp's Terms of
          Service, and WhatsApp may restrict or permanently ban the number you link.
        </p>
        <p>
          Broadcasts, scheduled messages and auto-replies raise that risk. Whatever happens to your account is your responsibility, so use
          Wahana wisely and don't link a number you can't afford to lose.
        </p>
      </div>
    </div>
  );
}
