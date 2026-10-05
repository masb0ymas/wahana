/** App-lock helpers: PIN hashing/verification and tuning constants. */

/** Keychain entry id used by `lib/secrets` to store the PIN hash. */
export const APP_LOCK_PIN_ID = "app-lock";

/** Selectable idle timeouts (minutes) before the app locks itself. */
export const LOCK_IDLE_OPTIONS = [1, 5, 10, 15, 30, 60] as const;

const PBKDF2_ITERATIONS = 120_000;
const SALT_BYTES = 16;

const enc = new TextEncoder();

function toB64(bytes: Uint8Array): string {
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin);
}

function fromB64(s: string): Uint8Array<ArrayBuffer> {
  return Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
}

async function derive(pin: string, salt: BufferSource): Promise<Uint8Array> {
  const key = await crypto.subtle.importKey("raw", enc.encode(pin), "PBKDF2", false, ["deriveBits"]);
  const bits = await crypto.subtle.deriveBits({ name: "PBKDF2", salt, iterations: PBKDF2_ITERATIONS, hash: "SHA-256" }, key, 256);
  return new Uint8Array(bits);
}

/** Hash a PIN into `"<base64 salt>:<base64 hash>"` for storage. */
export async function hashPin(pin: string): Promise<string> {
  const salt = crypto.getRandomValues(new Uint8Array(SALT_BYTES));
  return `${toB64(salt)}:${toB64(await derive(pin, salt))}`;
}

function equals(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a[i]! ^ b[i]!;
  return diff === 0;
}

/** Verify a PIN against a stored `hashPin` value. Returns false on any malformed input. */
export async function verifyPin(pin: string, stored: string): Promise<boolean> {
  const [salt, hash] = stored.split(":");
  if (!salt || !hash) return false;
  try {
    return equals(await derive(pin, fromB64(salt)), fromB64(hash));
  } catch {
    return false;
  }
}
