import { useState } from "react";
import { ArrowRight, Smartphone } from "lucide-react";
import { useWhatsApp } from "@/store/whatsapp";
import { cn } from "@/lib/utils";
import { UnofficialNotice } from "@/components/UnofficialNotice";

/** Ask App to show the welcome screen again (Settings → About). */
export const openWelcome = () => window.dispatchEvent(new CustomEvent("wahana:open-welcome"));

/**
 * First-run screen: shown while the app has no WhatsApp account linked. Link a device by
 * scanning the QR code, or skip to Settings.
 */
export function WelcomeScreen({ onDone }: { onDone: (tab: "chats" | "settings") => void }) {
  const addWa = useWhatsApp((s) => s.add);
  // Linking waits until the risk notice above has been acknowledged.
  const [accepted, setAccepted] = useState(false);
  return (
    <div className="flex-1 overflow-auto grid place-items-center p-6">
      <div className="w-full max-w-2xl space-y-6">
        <div className="text-center space-y-2">
          <img src="/logo.png" alt="" className="w-16 h-16 mx-auto rounded-2xl shadow" />
          <h1 className="text-2xl font-semibold">Welcome to Wahana</h1>
          <p className="text-sm text-neutral-500">
            An unofficial desktop client for your WhatsApp account. Link your phone to get started.
          </p>
        </div>

        <div className="max-w-md mx-auto space-y-3">
          <UnofficialNotice />
          <label className="flex items-start gap-2 text-sm cursor-pointer">
            <input type="checkbox" className="mt-0.5" checked={accepted} onChange={(e) => setAccepted(e.target.checked)} />
            <span>I understand Wahana is unofficial and that I'm responsible for any risk to my account, including a ban.</span>
          </label>
        </div>

        <div className="max-w-sm mx-auto">
          <Card
            icon={Smartphone}
            title="Link WhatsApp"
            body="Scan the QR code with your phone to link this app as a device. Add more accounts later."
            action={accepted ? "Link an account" : "Tick the box above to continue"}
            disabled={!accepted}
            onClick={() => {
              void addWa("WhatsApp").catch(console.error);
              onDone("chats");
            }}
          />
        </div>

        <p className="text-center text-xs text-neutral-500">
          Know your way around?{" "}
          <button className="underline hover:text-wa-dark" onClick={() => onDone("settings")}>
            Skip to Settings
          </button>
          . You can bring this screen back from Settings → About.
        </p>
      </div>
    </div>
  );
}

function Card({
  icon: Icon,
  title,
  body,
  action,
  disabled,
  onClick,
}: {
  icon: typeof Smartphone;
  title: string;
  body: string;
  action: string;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={cn(
        "group w-full text-left rounded-2xl border border-neutral-200 dark:border-neutral-800 bg-white dark:bg-neutral-900 p-5 space-y-3",
        "enabled:hover:border-wa-dark enabled:hover:shadow-md transition disabled:opacity-50 disabled:cursor-not-allowed",
      )}
    >
      <Icon size={24} className="text-wa-dark" />
      <h2 className="font-semibold">{title}</h2>
      <p className="text-sm text-neutral-500">{body}</p>
      <span className="inline-flex items-center gap-1 text-sm font-medium text-wa-dark">
        {action} <ArrowRight size={14} className="transition group-enabled:group-hover:translate-x-0.5" />
      </span>
    </button>
  );
}
