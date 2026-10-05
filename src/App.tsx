import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
  MessageSquare,
  Smartphone,
  Settings as SettingsIcon,
  Loader2,
  Download,
  X,
  CircleDashed,
  Sparkles,
  Images,
  LayoutGrid,
  RotateCw,
  Lock,
} from "lucide-react";
import { useSettings } from "@/store/settings";
import { useAppLock } from "@/store/appLock";
import { SettingsScreen } from "@/screens/SettingsScreen";
import { WelcomeScreen } from "@/screens/WelcomeScreen";
import { AccountsScreen } from "@/screens/AccountsScreen";
import { ChatScreen } from "@/screens/ChatScreen";
import { GridScreen } from "@/screens/GridScreen";
import { StatusScreen } from "@/screens/StatusScreen";
import { FeaturesScreen } from "@/screens/FeaturesScreen";
import { MediaScreen } from "@/screens/MediaScreen";
import { useBroadcastRunner } from "@/realtime/useBroadcastRunner";
import { pruneLogs } from "@/store/scheduler";
import { useAutoLabel } from "@/realtime/useAutoLabel";
import { useScheduler } from "@/realtime/useScheduler";
import { useWhatsApp } from "@/store/whatsapp";
import { resolveStyle, useAccountStyle } from "@/store/accountStyle";
import { useAutoReply } from "@/realtime/useAutoReply";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { ConfirmHost } from "@/components/Confirm";
import { LockScreen } from "@/components/LockScreen";
import { useIdleLock } from "@/realtime/useIdleLock";
import { useLockOnHide } from "@/realtime/useLockOnHide";
import { useRevoked } from "@/store/revoked";
import { usePins } from "@/store/pins";
import { useDrafts } from "@/store/drafts";
import { useChatPrefs } from "@/store/chatPrefs";
import { cn } from "@/lib/utils";
import { useBadge } from "@/realtime/useBadge";
import { useReactions } from "@/store/reactions";
import { useUpdater } from "@/realtime/useUpdater";
import { Button } from "@/components/ui";
import { nativeAccountKey } from "@/lib/account";
import { migrateUnownedToAccount } from "@/lib/migrateScope";

type Tab = "chats" | "grid" | "status" | "features" | "media" | "accounts" | "settings";

export default function App() {
  const { hydrated, hydrate } = useSettings();
  const appLockEnabled = useSettings((s) => s.appLockEnabled);
  const appLockPinSet = useAppLock((s) => s.pinSet);
  const appLocked = useAppLock((s) => s.locked);
  const hydrateAppLock = useAppLock((s) => s.hydrate);
  const waHydrated = useWhatsApp((s) => s.hydrated);
  const waAccounts = useWhatsApp((s) => s.accounts);
  // First run: nothing configured at all. `null` until both stores have loaded so the
  // screen neither flashes for existing users nor is skipped for new ones.
  const [welcome, setWelcome] = useState<boolean | null>(null);
  const [tab, setTab] = useState<Tab>("chats");
  const hydrateReactions = useReactions((s) => s.hydrate);
  const hydrateRevoked = useRevoked((s) => s.hydrate);
  const hydratePins = usePins((s) => s.hydrate);
  const hydrateDrafts = useDrafts((s) => s.hydrate);
  const hydrateChatPrefs = useChatPrefs((s) => s.hydrate);
  const hydrateWa = useWhatsApp((s) => s.hydrate);
  const hydrateAccountStyles = useAccountStyle((s) => s.hydrate);
  const styles = useAccountStyle((s) => s.styles);
  const accountGroups = waAccounts.map((a) => ({ id: a.id, name: a.name, unread: a.unread, ...resolveStyle(styles, a.id) }));
  const unreadGroups = accountGroups.filter((g) => g.unread > 0);
  useBadge(accountGroups);
  const chatsTitle = unreadGroups.length ? `Chats (⌘1) — ${unreadGroups.map((g) => `${g.name}: ${g.unread}`).join(" · ")}` : "Chats (⌘1)";
  const updater = useUpdater();
  useScheduler();
  useBroadcastRunner();
  useAutoReply();
  useAutoLabel();
  useIdleLock();
  useLockOnHide();
  // Everything is per account now: move legacy global Knowledge/quick-reply rows to the first
  // account once, so they are not orphaned. Idempotent, so it also covers a restored backup.
  const qc = useQueryClient();
  const migratedScope = useRef(false);
  useEffect(() => {
    if (migratedScope.current || !waHydrated || waAccounts.length === 0) return;
    migratedScope.current = true;
    void migrateUnownedToAccount(nativeAccountKey(waAccounts[0]!.id))
      .then(() => {
        qc.invalidateQueries({ queryKey: ["kb-docs"] });
        qc.invalidateQueries({ queryKey: ["quick-replies"] });
      })
      .catch((e) => console.warn("scope migration failed", e));
  }, [waHydrated, waAccounts, qc]);
  useEffect(() => {
    pruneLogs().catch((e) => console.warn("log pruning failed", e));
  }, []);

  useEffect(() => {
    void hydrate();
    void hydrateReactions();
    void hydrateRevoked();
    void hydratePins();
    void hydrateDrafts();
    void hydrateChatPrefs();
    void hydrateAccountStyles();
    void hydrateAppLock();
    hydrateWa().catch(console.error);
  }, [
    hydrate,
    hydrateReactions,
    hydrateRevoked,
    hydratePins,
    hydrateDrafts,
    hydrateChatPrefs,
    hydrateAccountStyles,
    hydrateWa,
    hydrateAppLock,
  ]);

  useEffect(() => {
    if (hydrated && waHydrated && welcome === null) setWelcome(waAccounts.length === 0);
  }, [hydrated, waHydrated, welcome, waAccounts.length]);

  // Lock on start: once the prefs and the PIN are known, gate the app behind the PIN.
  useEffect(() => {
    if (hydrated && appLockEnabled && appLockPinSet && useSettings.getState().appLockOnStart) useAppLock.getState().lock();
  }, [hydrated, appLockEnabled, appLockPinSet]);

  // Global shortcuts: ⌘/Ctrl+1–6 switch tabs, ⌘/Ctrl+K focus chat search, ⌘/Ctrl+, opens settings.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // While locked, swallow global shortcuts so ⌘R cannot reload away the lock.
      if (useAppLock.getState().locked) return;
      const mod = e.metaKey || e.ctrlKey;
      if (!mod) return;
      const tabs: Record<string, Tab> = {
        "1": "chats",
        "2": "status",
        "3": "features",
        "4": "accounts",
        "5": "settings",
        "6": "media",
        "7": "grid",
      };
      if (tabs[e.key]) {
        e.preventDefault();
        setTab(tabs[e.key]!);
      } else if (e.key === "k") {
        e.preventDefault();
        setTab("chats");
        setTimeout(() => window.dispatchEvent(new CustomEvent("wahana:focus-search")), 0);
      } else if (e.key === ",") {
        e.preventDefault();
        setTab("settings");
      } else if (e.key === "r") {
        e.preventDefault();
        window.location.reload();
      }
    };
    const onOpenAccounts = () => setTab("accounts");
    const onOpenStatus = () => setTab("status");
    const onOpenWelcome = () => setWelcome(true);
    window.addEventListener("keydown", onKey);
    window.addEventListener("wahana:open-accounts", onOpenAccounts);
    window.addEventListener("wahana:open-status", onOpenStatus);
    window.addEventListener("wahana:open-welcome", onOpenWelcome);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("wahana:open-accounts", onOpenAccounts);
      window.removeEventListener("wahana:open-status", onOpenStatus);
      window.removeEventListener("wahana:open-welcome", onOpenWelcome);
    };
  }, []);

  if (!hydrated) {
    return (
      <div className="h-full grid place-items-center text-neutral-500">
        <Loader2 className="animate-spin" />
      </div>
    );
  }

  const nav: { id: Tab; icon: typeof MessageSquare; label: string }[] = [
    { id: "chats", icon: MessageSquare, label: chatsTitle },
    { id: "grid", icon: LayoutGrid, label: "Multi-account (⌘7)" },
    { id: "status", icon: CircleDashed, label: "Status (⌘2)" },
    { id: "features", icon: Sparkles, label: "Features: Scheduler, Broadcast, Auto-reply, Tweaks (⌘3)" },
    { id: "media", icon: Images, label: "Media saved on this computer (⌘6)" },
    { id: "accounts", icon: Smartphone, label: "Accounts (⌘4)" },
    { id: "settings", icon: SettingsIcon, label: "Settings (⌘5)" },
  ];

  return (
    <div className="h-full flex">
      <ConfirmHost />
      {appLocked && <LockScreen />}
      <aside className="w-16 shrink-0 flex flex-col items-center py-4 gap-2 bg-wa-teal text-white/80">
        {nav.map((n) => (
          <button
            key={n.id}
            title={n.label}
            onClick={() => {
              setWelcome(false);
              setTab(n.id);
            }}
            className={cn(
              "relative w-11 h-11 rounded-xl grid place-items-center hover:bg-white/10 transition",
              tab === n.id && "bg-white/20 text-white",
            )}
          >
            <n.icon size={22} />
            {n.id === "chats" && unreadGroups.length > 0 && (
              <span className="absolute -top-1 -right-1 flex flex-row-reverse items-center gap-0.5">
                {unreadGroups.slice(0, 3).map((g) => (
                  <span
                    key={g.id}
                    title={`${g.name}: ${g.unread}`}
                    style={{ backgroundColor: g.color }}
                    className="min-w-[16px] h-[16px] px-1 rounded-full text-[10px] font-bold text-white grid place-items-center shadow"
                  >
                    {g.unread > 99 ? "99+" : g.unread}
                  </span>
                ))}
                {unreadGroups.length > 3 && (
                  <span className="min-w-[16px] h-[16px] px-1 rounded-full bg-neutral-500 text-[10px] font-bold text-white grid place-items-center shadow">
                    +{unreadGroups.length - 3}
                  </span>
                )}
              </span>
            )}
          </button>
        ))}
        <div className="mt-auto flex flex-col items-center gap-2">
          {appLockEnabled && appLockPinSet && (
            <button
              title="Lock now"
              aria-label="Lock now"
              onClick={() => useAppLock.getState().lock()}
              className="w-11 h-11 rounded-xl grid place-items-center hover:bg-white/10 transition"
            >
              <Lock size={20} />
            </button>
          )}
          <button
            title="Reload (⌘R)"
            aria-label="Reload"
            onClick={() => window.location.reload()}
            className="w-11 h-11 rounded-xl grid place-items-center hover:bg-white/10 transition"
          >
            <RotateCw size={20} />
          </button>
        </div>
      </aside>
      <main className="flex-1 min-w-0 flex flex-col">
        {updater.update && (
          <div className="shrink-0 flex items-center gap-3 px-4 py-2 bg-wa-dark text-white text-sm">
            <Download size={16} />
            <span className="flex-1">
              Wahana {updater.update.version} is available.
              {updater.progress !== null && ` Downloading… ${Math.round(updater.progress * 100)}%`}
              {updater.error && <span className="ml-2 text-red-200 selectable">{updater.error}</span>}
            </span>
            <Button size="sm" variant="secondary" onClick={updater.install} disabled={updater.progress !== null}>
              Install & restart
            </Button>
            <button onClick={updater.dismiss} title="Later">
              <X size={16} />
            </button>
          </div>
        )}
        <div className="flex-1 min-h-0 flex">
          <ErrorBoundary key={welcome ? "welcome" : tab} label={welcome ? "welcome" : tab}>
            {welcome && (
              <WelcomeScreen
                onDone={(t) => {
                  setWelcome(false);
                  setTab(t);
                }}
              />
            )}
            {!welcome && tab === "chats" && <ChatScreen />}
            {!welcome && tab === "grid" && <GridScreen />}
            {!welcome && tab === "status" && <StatusScreen />}
            {!welcome && tab === "features" && <FeaturesScreen />}
            {!welcome && tab === "media" && <MediaScreen />}
            {!welcome && tab === "accounts" && <AccountsScreen />}
            {!welcome && tab === "settings" && <SettingsScreen />}
          </ErrorBoundary>
        </div>
      </main>
    </div>
  );
}
