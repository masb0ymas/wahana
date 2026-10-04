import { db } from "@/store/scheduler";

export interface QuickReply {
  id: string;
  /** Account this reply belongs to (`native:<id>`), or null for a legacy entry not yet reassigned. */
  account: string | null;
  shortcut: string;
  text: string;
  created_at: number;
}

/** All quick replies (Settings/backup), or — with an account — only that account's own. */
export const listQuickReplies = async (account?: string) =>
  account === undefined
    ? (await db()).select<QuickReply[]>("SELECT * FROM quick_replies ORDER BY shortcut")
    : (await db()).select<QuickReply[]>("SELECT * FROM quick_replies WHERE account = $1 ORDER BY shortcut", [account]);

/** Legacy global replies have no owner once every reply is per account: move them to one account. */
export async function reassignUnownedQuickRepliesToAccount(account: string) {
  await (await db()).execute("UPDATE quick_replies SET account = $1 WHERE account IS NULL", [account]);
}

export async function saveQuickReply(r: Omit<QuickReply, "created_at">) {
  await (
    await db()
  ).execute(
    "INSERT INTO quick_replies (id, account, shortcut, text, created_at) VALUES ($1,$2,$3,$4,$5) ON CONFLICT(id) DO UPDATE SET account=excluded.account, shortcut=excluded.shortcut, text=excluded.text",
    [r.id, r.account || null, r.shortcut.replace(/^\//, "").trim().toLowerCase(), r.text, Math.floor(Date.now() / 1000)],
  );
}

export const deleteQuickReply = async (id: string) => (await db()).execute("DELETE FROM quick_replies WHERE id = $1", [id]);

/** Expand template variables against the current chat. */
export function expandTemplate(text: string, ctx: { name?: string; phone?: string }) {
  const now = new Date();
  return text
    .replace(/\{name\}/gi, ctx.name ?? "")
    .replace(/\{phone\}/gi, ctx.phone ?? "")
    .replace(/\{time\}/gi, now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }))
    .replace(/\{date\}/gi, now.toLocaleDateString());
}
