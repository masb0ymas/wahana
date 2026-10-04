import { Avatar } from "@/components/ui";
import { usePicture } from "@/screens/whatsapp/usePicture";
import { WhatsAppScreen } from "@/screens/WhatsAppScreen";
import { type NativeAccount, type NativeWaStatus } from "@/lib/nativeWa";
import { cn } from "@/lib/utils";

const dotTone: Record<NativeWaStatus, string> = {
  working: "bg-wa",
  starting: "bg-amber-400",
  qr: "bg-sky-400",
  stopped: "bg-neutral-400",
  logged_out: "bg-red-500",
  failed: "bg-red-500",
};

const tileShell =
  "flex flex-col min-h-0 h-full rounded-xl border border-neutral-200 dark:border-neutral-800 bg-white dark:bg-neutral-900 overflow-hidden";

/**
 * One account in the grid: a header strip and a full `WhatsAppScreen` in embedded mode, so the
 * chat list and conversation behave exactly like the regular Chats tab (search, filters, pin,
 * mute, labels, context menu, read-all, select).
 */
export function AccountChatTile({ account }: { account: NativeAccount }) {
  const connected = account.status === "working";
  // The account's own profile picture, looked up from its own JID.
  const selfPicture = usePicture(account.id, account.me?.id ?? "", connected && !!account.me);

  return (
    <div className={tileShell}>
      <div className="shrink-0 flex items-center gap-2 px-3 py-2 border-b border-neutral-200 dark:border-neutral-800">
        <span className={cn("w-2 h-2 rounded-full shrink-0", dotTone[account.status])} title={account.status} />
        <Avatar src={selfPicture} name={account.me?.pushName ?? account.name} size={26} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium">{account.name}</div>
          {account.me?.pushName && <div className="truncate text-[11px] text-neutral-500">~{account.me.pushName}</div>}
        </div>
        {account.unread > 0 && (
          <span className="shrink-0 min-w-[18px] h-[18px] px-1 rounded-full bg-wa text-[10px] font-bold text-white grid place-items-center">
            {account.unread}
          </span>
        )}
      </div>
      <WhatsAppScreen account={account} header={null} embedded />
    </div>
  );
}
