import { Avatar } from "@/components/ui";
import { usePicture } from "@/screens/whatsapp/usePicture";
import { useWhatsApp } from "@/store/whatsapp";

/**
 * Avatar for a chat: loads its WhatsApp profile picture (shared app-wide cache) and falls
 * back to initials. Pass an empty `accountId` to always show initials.
 */
export function ChatAvatar({ accountId, chatId, name, size = 40 }: { accountId: string; chatId: string; name: string; size?: number }) {
  const connected = useWhatsApp((s) => s.accounts.find((a) => a.id === accountId)?.status === "working");
  const src = usePicture(accountId, chatId, connected);
  return <Avatar src={src} name={name} size={size} />;
}
