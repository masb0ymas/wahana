import { describe, expect, it } from "vitest";
import { queueMediaDownload } from "@/lib/mediaQueue";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((r) => (resolve = r));
  return { promise, resolve };
}

describe("queueMediaDownload", () => {
  it("runs at most three downloads at once", async () => {
    let active = 0;
    let peak = 0;
    const gates = Array.from({ length: 10 }, deferred);
    const tasks = gates.map((g) =>
      queueMediaDownload(async () => {
        active++;
        peak = Math.max(peak, active);
        await g.promise;
        active--;
      }),
    );

    await Promise.resolve();
    expect(peak).toBe(3);

    for (const g of gates) g.resolve();
    await Promise.all(tasks);
    expect(peak).toBe(3);
  });

  it("starts queued downloads in order", async () => {
    const order: number[] = [];
    const gates = Array.from({ length: 5 }, deferred);
    const tasks = gates.map((g, i) =>
      queueMediaDownload(async () => {
        order.push(i);
        await g.promise;
      }),
    );

    await Promise.resolve();
    for (const g of gates) g.resolve();
    await Promise.all(tasks);
    expect(order).toEqual([0, 1, 2, 3, 4]);
  });

  it("lets a priority download jump the queue", async () => {
    const order: string[] = [];
    const block = deferred();
    const running = [0, 1, 2].map(() => queueMediaDownload(async () => void (await block.promise)));
    const normal = queueMediaDownload(async () => void order.push("normal"));
    const urgent = queueMediaDownload(async () => void order.push("urgent"), true);

    block.resolve();
    await Promise.all([...running, normal, urgent]);
    expect(order).toEqual(["urgent", "normal"]);
  });

  it("rejects when the task rejects and frees the slot", async () => {
    const failing = queueMediaDownload(async () => {
      throw new Error("nope");
    });
    await expect(failing).rejects.toThrow("nope");

    const block = deferred();
    const running = queueMediaDownload(async () => void (await block.promise));
    const after = queueMediaDownload(async () => "ok");
    block.resolve();
    await expect(running).resolves.toBeUndefined();
    await expect(after).resolves.toBe("ok");
  });
});
