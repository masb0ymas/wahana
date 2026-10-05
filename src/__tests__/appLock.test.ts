import { describe, expect, it } from "vitest";
import { hashPin, verifyPin } from "@/lib/appLock";

describe("hashPin", () => {
  it("produces a salt:hash pair", async () => {
    const stored = await hashPin("1234");
    const [salt, hash] = stored.split(":");
    expect(salt).toBeTruthy();
    expect(hash).toBeTruthy();
    expect(salt).not.toBe(hash);
  });

  it("salts each call, so the same PIN hashes differently", async () => {
    expect(await hashPin("1234")).not.toBe(await hashPin("1234"));
  });
});

describe("verifyPin", () => {
  it("accepts the matching PIN and rejects others", async () => {
    const stored = await hashPin("1234");
    expect(await verifyPin("1234", stored)).toBe(true);
    expect(await verifyPin("4321", stored)).toBe(false);
    expect(await verifyPin("", stored)).toBe(false);
  });

  it("returns false for malformed stored values", async () => {
    expect(await verifyPin("1234", "")).toBe(false);
    expect(await verifyPin("1234", "garbage")).toBe(false);
    expect(await verifyPin("1234", "!!!:???")).toBe(false);
  });
});
