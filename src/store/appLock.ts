import { create } from "zustand";
import { APP_LOCK_PIN_ID, hashPin, verifyPin } from "@/lib/appLock";
import { deleteSecret, getSecret, setSecret } from "@/lib/secrets";

/** Wrong PINs allowed before a cooldown kicks in. */
const MAX_ATTEMPTS = 5;
const BASE_LOCKOUT_MS = 30_000;
const MAX_LOCKOUT_MS = 5 * 60_000;

interface AppLockState {
  hydrated: boolean;
  /** Whether a PIN has been set (the hash lives in the OS keychain). */
  pinSet: boolean;
  /** In-memory only: a reload/restart clears the lock (matching "no lock on start"). */
  locked: boolean;
  failedAttempts: number;
  /** Epoch ms until which PIN entry is blocked; 0 = not blocked. */
  lockoutUntil: number;
  hydrate: () => Promise<void>;
  lock: () => void;
  unlock: () => void;
  setPin: (pin: string) => Promise<void>;
  clearPin: () => Promise<void>;
  verify: (pin: string) => Promise<boolean>;
}

export const useAppLock = create<AppLockState>((set, get) => ({
  hydrated: false,
  pinSet: false,
  locked: false,
  failedAttempts: 0,
  lockoutUntil: 0,

  async hydrate() {
    const pinSet = (await getSecret(APP_LOCK_PIN_ID)) !== "";
    set({ hydrated: true, pinSet });
  },

  lock() {
    if (!get().pinSet) return;
    set({ locked: true, failedAttempts: 0, lockoutUntil: 0 });
  },

  unlock() {
    set({ locked: false, failedAttempts: 0, lockoutUntil: 0 });
  },

  async setPin(pin) {
    await setSecret(APP_LOCK_PIN_ID, await hashPin(pin));
    set({ pinSet: true });
  },

  async clearPin() {
    await deleteSecret(APP_LOCK_PIN_ID);
    set({ pinSet: false, locked: false, failedAttempts: 0, lockoutUntil: 0 });
  },

  async verify(pin) {
    if (Date.now() < get().lockoutUntil) return false;
    const stored = await getSecret(APP_LOCK_PIN_ID);
    if (stored && (await verifyPin(pin, stored))) {
      get().unlock();
      return true;
    }
    const failed = get().failedAttempts + 1;
    const lockoutUntil = failed >= MAX_ATTEMPTS ? Date.now() + Math.min(BASE_LOCKOUT_MS * 2 ** (failed - MAX_ATTEMPTS), MAX_LOCKOUT_MS) : 0;
    set({ failedAttempts: failed, lockoutUntil });
    return false;
  },
}));
