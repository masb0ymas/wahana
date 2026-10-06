import { db } from "@/store/scheduler";

/** A named message picked from the composer's "+" menu; `{{name}}` and `{{phone}}` are filled per chat. */
export interface Template {
  id: string;
  /** Account it belongs to (`native:<id>`). */
  account: string;
  name: string;
  text: string;
  /** Times it was put into a message; the picker lists the most used first. */
  uses: number;
  /** When it was last used (unix seconds), or null. */
  last_used: number | null;
  created_at: number;
}

/** The variables a template can hold, filled from the chat it is used in. */
export const TEMPLATE_VARIABLES = [
  { token: "{{name}}", hint: "Their name (the group's name in a group)" },
  { token: "{{phone}}", hint: "Their phone number" },
] as const;

/** All templates (backup), or — with an account — that account's, most used first. */
export const listTemplates = async (account?: string) =>
  account === undefined
    ? (await db()).select<Template[]>("SELECT * FROM templates ORDER BY name")
    : (await db()).select<Template[]>("SELECT * FROM templates WHERE account = $1 ORDER BY uses DESC, name COLLATE NOCASE", [account]);

export async function saveTemplate(t: Pick<Template, "id" | "account" | "name" | "text">) {
  await (
    await db()
  ).execute(
    "INSERT INTO templates (id, account, name, text, created_at) VALUES ($1,$2,$3,$4,$5) ON CONFLICT(id) DO UPDATE SET account=excluded.account, name=excluded.name, text=excluded.text",
    [t.id, t.account, t.name.trim(), t.text.trim(), Math.floor(Date.now() / 1000)],
  );
}

export const deleteTemplate = async (id: string) => (await db()).execute("DELETE FROM templates WHERE id = $1", [id]);

export const markTemplateUsed = async (id: string) =>
  (await db()).execute("UPDATE templates SET uses = uses + 1, last_used = $2 WHERE id = $1", [id, Math.floor(Date.now() / 1000)]);

/** Why a template cannot be saved, or null when it can. */
export function templateError(name: string, text: string): string | null {
  if (!name.trim()) return "Give the template a name.";
  if (!text.trim()) return "The message is empty.";
  return null;
}

/** Fills `{{name}}` and `{{phone}}` (spaces inside the braces and any case allowed); unknown values become empty. */
export function fillTemplate(text: string, ctx: { name?: string | null; phone?: string | null }) {
  return text.replace(/\{\{\s*(name|phone)\s*\}\}/gi, (_, key: string) => (key.toLowerCase() === "name" ? ctx.name : ctx.phone) ?? "");
}
