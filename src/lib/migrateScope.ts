import { invalidateKbCache } from "@/lib/knowledge";
import { reassignUnownedToAccount } from "@/store/knowledge";
import { reassignUnownedQuickRepliesToAccount } from "@/store/quickReplies";

/**
 * Every Knowledge entry and quick reply is owned by exactly one account now. Rows created under the
 * removed "All accounts" scope (`account IS NULL`) are moved to `account`. Idempotent: once no NULL
 * rows remain this is a no-op, so it is safe to run on every launch and after a backup restore.
 */
export async function migrateUnownedToAccount(account: string) {
  await reassignUnownedToAccount(account);
  await reassignUnownedQuickRepliesToAccount(account);
  invalidateKbCache();
}
