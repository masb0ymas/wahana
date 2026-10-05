import { displayId, isChannel } from "@/lib/utils";

/** The parts of a chat needed to label it: its resolved name, whether it is from contacts, and its number. */
export interface ChatLabelSource {
  name: string;
  saved: boolean;
  phone: string | null;
}

/**
 * How WhatsApp itself labels a chat: a saved contact by its name; anyone else by their
 * number, with the name they gave themselves as "~name".
 */
export function chatLabel(chat: ChatLabelSource | undefined, chatId: string) {
  if (!chat) return { title: isChannel(chatId) ? displayId(chatId) : chatId.split("@")[0]!, pushName: null };
  if (chat.saved || !chat.phone) {
    // A channel with no name yet falls back to "Channel 123456", not its raw id.
    const unnamed = isChannel(chatId) && chat.name === chatId.split("@")[0];
    return { title: unnamed ? displayId(chatId) : chat.name, pushName: null };
  }
  return { title: chat.phone, pushName: chat.name !== chat.phone ? chat.name : null };
}
