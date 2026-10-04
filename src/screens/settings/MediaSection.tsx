import { useContext } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useSettings, type MediaPrefs } from "@/store/settings";
import { Button } from "@/components/ui";
import { ScopeCtx, Toggle } from "./shared";

/** The auto-download switches, shown per account (the "Apply to" picker above picks which). */
const KINDS: { key: keyof MediaPrefs; label: string; hint?: string }[] = [
  { key: "autoLoadImages", label: "Auto-load images", hint: "Photos appear immediately." },
  { key: "autoLoadStickers", label: "Auto-load stickers", hint: "Detected from the message type (stickers are webp images)." },
  { key: "autoLoadVideos", label: "Auto-load videos", hint: "Videos can be large; off shows a blurred frame with the size." },
  { key: "autoLoadAudio", label: "Auto-load voice notes & audio" },
];

export function MediaSection() {
  const s = useSettings();
  const { scope } = useContext(ScopeCtx);
  const qc = useQueryClient();

  const override = s.mediaByAccount[scope] ?? {};
  const overridden = Object.keys(override).length > 0;

  const refresh = () => qc.invalidateQueries({ queryKey: ["messages"] });
  const set = async (key: keyof MediaPrefs, v: boolean) => {
    await s.save({ mediaByAccount: { ...s.mediaByAccount, [scope]: { ...override, [key]: v } } });
    // Message lists are fetched with the auto-download list; refetch so placeholders update.
    void refresh();
  };
  const reset = () => {
    const mediaByAccount = { ...s.mediaByAccount };
    delete mediaByAccount[scope];
    void s.save({ mediaByAccount }).then(refresh);
  };

  if (!scope) return <p className="text-sm text-neutral-500">Connect a WhatsApp account first to change media downloads.</p>;

  return (
    <>
      <div className="flex items-center gap-2 text-xs text-neutral-500">
        <span className="flex-1">{overridden ? "This account has its own values." : "This account uses the defaults."}</span>
        {overridden && (
          <Button size="sm" variant="ghost" onClick={reset}>
            Reset to default
          </Button>
        )}
      </div>
      {KINDS.map((k) => (
        <Toggle key={k.key} label={k.label} hint={k.hint} checked={override[k.key] ?? s[k.key]} onChange={(v) => void set(k.key, v)} />
      ))}
      <p className="text-xs text-neutral-500">Documents are never downloaded automatically — click to open.</p>
    </>
  );
}
