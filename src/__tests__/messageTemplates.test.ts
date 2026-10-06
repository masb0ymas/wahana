import { describe, expect, it } from "vitest";
import { normalizeShortcut, quickReplyError, type QuickReply } from "@/store/quickReplies";
import { fillTemplate } from "@/store/templates";

const reply = (id: string, shortcut: string): QuickReply => ({
  id,
  account: "native:a",
  shortcut,
  text: "x",
  created_at: 0,
});

describe("quick reply validation", () => {
  it("stores a shortcut as one lowercase word without the slash", () => {
    expect(normalizeShortcut(" /Thanks ")).toBe("thanks");
  });

  it("rejects a missing or spaced shortcut and empty text", () => {
    expect(quickReplyError("", "hi", [])).toMatch(/one word/);
    expect(quickReplyError("two words", "hi", [])).toMatch(/one word/);
    expect(quickReplyError("hi", "  ", [])).toMatch(/required/);
  });

  it("rejects a shortcut another template already uses, but not the one being edited", () => {
    const others = [reply("1", "thanks")];
    expect(quickReplyError("/Thanks", "hi", others)).toMatch(/already used/);
    expect(quickReplyError("thanks", "hi", others, "1")).toBeNull();
  });
});

describe("fillTemplate", () => {
  it("fills {{name}} and {{phone}}, with spaces inside the braces and any case", () => {
    expect(fillTemplate("Halo {{name}}, nomor {{ Phone }}", { name: "Budi", phone: "+62812" })).toBe("Halo Budi, nomor +62812");
  });

  it("leaves unknown values empty and other braces alone", () => {
    expect(fillTemplate("Hi {{name}} {{other}} {name}", { phone: "+1" })).toBe("Hi  {{other}} {name}");
  });
});
