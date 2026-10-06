import { describe, expect, it } from "vitest";
import { mentionChatIds } from "@/lib/mentions";
import type { NativeChatDetails, NativeGroupMember } from "@/lib/nativeWa";

const member = (id: string, phone: string | null, isMe = false): NativeGroupMember => ({
  id,
  name: null,
  saved: false,
  phone,
  admin: false,
  superAdmin: false,
  isMe,
});

const group = {
  type: "group",
  members: [member("111222333444555@lid", "+62 812-3456-789"), member("999888777666555@lid", "628000", true)],
} as unknown as NativeChatDetails;

describe("mentionChatIds", () => {
  it("opens a member mentioned by privacy id, with their phone as the alternative", () => {
    expect(mentionChatIds(group, "111222333444555")).toEqual(["111222333444555@lid", "628123456789@s.whatsapp.net"]);
  });

  it("finds a member mentioned by phone number", () => {
    expect(mentionChatIds(group, "628123456789")?.[0]).toBe("111222333444555@lid");
  });

  it("does not open yourself or someone outside the group", () => {
    expect(mentionChatIds(group, "999888777666555")).toBeNull();
    expect(mentionChatIds(group, "123456789012345")).toBeNull();
    expect(mentionChatIds(undefined, "111222333444555")).toBeNull();
  });
});
