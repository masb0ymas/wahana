import { useEffect, useRef } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { postStatusMediaOn, postStatusTextOn, sendMediaOn, sendTextOn } from "@/lib/send";
import { useSettings } from "@/store/settings";
import { claimRun, dueSchedules, listScheduleMedia, nextOccurrence, recordRun, type Schedule, type ScheduleMedia } from "@/store/scheduler";
import { sendNotification } from "@tauri-apps/plugin-notification";
import { errMsg } from "@/lib/utils";

/** Jobs later than this are marked missed instead of sent (app was closed). */
export const GRACE_SECONDS = 60 * 60;
/** How often due schedules are looked up. */
const DUE_CHECK_MS = 20_000;
/** Pump cadence; a pending attachment goes out on the first tick past its gap. */
const TICK_MS = 1_000;
/** Random spacing between the attachments of one batch (anti-spam). */
const GAP_MIN_MS = 3_000;
const GAP_MAX_MS = 8_000;

interface QueuedItem {
  s: Schedule;
  media: ScheduleMedia | null;
  caption: string;
}

interface Batch {
  s: Schedule;
  total: number;
  done: number;
  errors: string[];
  messageId?: string;
}

/** Send one queued item; returns the created message id when there is one. */
async function sendItem(job: QueuedItem): Promise<string | undefined> {
  const { s, media, caption } = job;
  if (s.target_type === "status") {
    if (media)
      await postStatusMediaOn(
        s.account,
        { mimetype: media.mime ?? "application/octet-stream", name: media.name ?? "file", base64: media.b64 },
        caption,
      );
    else await postStatusTextOn(s.account, caption);
    return undefined;
  }
  const chatId = s.target_id!;
  if (media) {
    const res = await sendMediaOn(
      s.account,
      chatId,
      { mimetype: media.mime ?? "application/octet-stream", name: media.name ?? "file", base64: media.b64 },
      caption,
    );
    return res.id;
  }
  return (await sendTextOn(s.account, chatId, caption)).id;
}

/** Runs due schedules of every account while the app is alive (window may be hidden in the tray). */
export function useScheduler() {
  const qc = useQueryClient();
  const busy = useRef(false);

  useEffect(() => {
    // A global FIFO: every attachment of every due schedule goes out one at a time, spaced.
    const queue: QueuedItem[] = [];
    const batches = new Map<string, Batch>();
    let nextAt = 0;
    let lastDueCheck = 0;

    const enqueueDue = async (now: number) => {
      const due = await dueSchedules(now);
      for (const s of due) {
        const late = now - s.next_run;
        const next = nextOccurrence(s, now);
        // Claim first, send second: never send twice (second instance, crash after send).
        if (!(await claimRun(s, next))) continue;
        if (late > GRACE_SECONDS) {
          await recordRun(s, "missed", { error: `Missed by ${Math.round(late / 60)} min (app was not running)` });
          continue;
        }
        const media = await listScheduleMedia(s.id);
        const text = s.text ?? "";
        // The caption rides on the first attachment only; without attachments it is a plain text message.
        const items: QueuedItem[] = media.length
          ? media.map((m, i) => ({ s, media: m, caption: i === 0 ? text : "" }))
          : [{ s, media: null, caption: text }];
        batches.set(s.id, { s, total: items.length, done: 0, errors: [] });
        queue.push(...items);
      }
    };

    const pump = async () => {
      const job = queue.shift()!;
      const batch = batches.get(job.s.id)!;
      try {
        const id = await sendItem(job);
        if (id && !batch.messageId) batch.messageId = id;
      } catch (e) {
        batch.errors.push(errMsg(e));
      }
      batch.done++;
      if (batch.done < batch.total) return;
      batches.delete(job.s.id);
      const error = batch.errors.length
        ? batch.errors.length > 1
          ? `${batch.errors.length} of ${batch.total} attachments failed: ${batch.errors[0]}`
          : batch.errors[0]
        : undefined;
      await recordRun(job.s, batch.errors.length ? "error" : "ok", { error, messageId: batch.messageId });
      if (batch.errors.length && useSettings.getState().notifications)
        sendNotification({
          title: "Scheduled message failed",
          body: `${job.s.target_name ?? job.s.target_id ?? "status"}: ${error}`.slice(0, 200),
        });
      qc.invalidateQueries({ queryKey: ["schedules"] });
    };

    const tick = async () => {
      if (busy.current) return;
      busy.current = true;
      try {
        const now = Date.now();
        if (now - lastDueCheck >= DUE_CHECK_MS) {
          lastDueCheck = now;
          await enqueueDue(Math.floor(now / 1000));
        }
        if (queue.length && now >= nextAt) {
          await pump();
          nextAt = Date.now() + GAP_MIN_MS + Math.random() * (GAP_MAX_MS - GAP_MIN_MS);
        }
      } catch (e) {
        console.warn("scheduler tick failed", e);
      } finally {
        busy.current = false;
      }
    };

    void tick();
    const t = setInterval(tick, TICK_MS);
    return () => clearInterval(t);
  }, [qc]);
}
