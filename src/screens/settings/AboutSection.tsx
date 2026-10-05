import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Bug, Code, Loader2, RefreshCw, ScrollText } from "lucide-react";
import { Button } from "@/components/ui";
import { openWelcome } from "@/screens/WelcomeScreen";
import { useWhatsApp } from "@/store/whatsapp";
import { useUpdaterStore } from "@/store/updater";

const REPO = "https://github.com/ashafizullah/wahana";
const LINKS = [
  { label: "Source code", url: REPO, icon: Code },
  { label: "Report a bug", url: `${REPO}/issues/new`, icon: Bug },
  { label: "Release notes", url: `${REPO}/releases`, icon: ScrollText },
];

export function AboutSection() {
  const native = useWhatsApp((s) => s.accounts);
  const [appVersion, setAppVersion] = useState("");
  useEffect(() => {
    getVersion()
      .then(setAppVersion)
      .catch(() => {});
  }, []);
  const status = useUpdaterStore((s) => s.status);
  const update = useUpdaterStore((s) => s.update);
  const progress = useUpdaterStore((s) => s.progress);
  const error = useUpdaterStore((s) => s.error);
  const checkNow = useUpdaterStore((s) => s.checkNow);
  const install = useUpdaterStore((s) => s.install);
  const linked = native.filter((a) => a.status === "working").length;
  const nativeText = native.length ? `${native.length} account${native.length === 1 ? "" : "s"} · ${linked} connected` : "No accounts";
  return (
    <>
      <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1 text-sm">
        <dt className="text-neutral-500">App</dt>
        <dd className="selectable">Wahana {appVersion}</dd>
        <dt className="text-neutral-500">Accounts</dt>
        <dd className="selectable">{nativeText}</dd>
      </dl>
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-sm">
        {LINKS.map(({ label, url, icon: Icon }) => (
          <button
            key={url}
            onClick={() => void openUrl(url)}
            title={url}
            className="inline-flex items-center gap-1.5 text-wa-dark hover:underline dark:text-wa"
          >
            <Icon size={14} /> {label}
          </button>
        ))}
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Button variant="secondary" size="sm" onClick={() => void checkNow()} disabled={status === "checking" || progress !== null}>
          {status === "checking" ? <Loader2 size={14} className="animate-spin" /> : <RefreshCw size={14} />}
          Check for updates
        </Button>
        {update && (
          <Button size="sm" onClick={() => void install()} disabled={progress !== null}>
            Install {update.version} &amp; restart
          </Button>
        )}
      </div>
      {(status !== "idle" || progress !== null) && (
        <p className={"text-xs " + (status === "error" ? "text-red-600 selectable" : "text-neutral-500")}>
          {status === "checking" && "Checking for updates…"}
          {status === "latest" && `You're on the latest version (${appVersion}).`}
          {status === "available" && update && `Wahana ${update.version} is available.`}
          {status === "error" && error}
          {progress !== null && ` Downloading… ${Math.round(progress * 100)}%`}
        </p>
      )}
      <p className="text-xs text-neutral-500">
        Unofficial client, not affiliated with WhatsApp or Meta. Unofficial clients may break WhatsApp's Terms of Service and can get your
        number banned; use at your own risk.
      </p>
      <Button variant="secondary" size="sm" onClick={openWelcome}>
        Show welcome screen
      </Button>
    </>
  );
}
