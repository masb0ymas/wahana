import { useEffect, useRef } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useSettings } from "@/store/settings";
import { sendMediaOn, sendTextOn } from "@/lib/send";
import {
  getBroadcast,
  listBroadcastMedia,
  listRunning,
  markItem,
  nextPending,
  setBroadcastStatus,
  type Broadcast,
  type BroadcastItem,
  type BroadcastMedia,
} from "@/store/broadcast";
import { expandTemplate } from "@/store/quickReplies";
import { sendNotification } from "@tauri-apps/plugin-notification";
import { errMsg } from "@/lib/utils";

/** Pause a broadcast after this many recipients fail in a row (session down, rate-limited…). */
export const MAX_CONSECUTIVE_ERRORS = 5;

interface SendJob {
  b: Broadcast;
  item: BroadcastItem;
  media: BroadcastMedia | null;
  caption: string;
  /** Last send for this recipient: mark the item done and apply the inter-recipient pause. */
  last: boolean;
  /** Accumulated over the recipient's sends. */
  state: { failed?: string; messageId?: string };
}

/**
 * Sends the next pending recipient of every `running` broadcast. A recipient with several
 * attachments gets them one per tick (~1s apart); the random delay is applied between
 * recipients. Runs while the app is alive.
 */
export function useBroadcastRunner() {
  const qc = useQueryClient();
  const busy = useRef(false);
  const nextAt = useRef<Record<string, number>>({});
  const errorStreak = useRef<Record<string, number>>({});
  const queues = useRef<Record<string, SendJob[]>>({});

  useEffect(() => {
    const tick = async () => {
      if (busy.current) return;
      busy.current = true;
      try {
        // Slim rows only: attachments are loaded per recipient below, not once a second.
        const running = await listRunning();
        for (const slim of running) {
          if ((nextAt.current[slim.id] ?? 0) > Date.now()) continue;
          let jobs = queues.current[slim.id];
          if (!jobs || jobs.length === 0) {
            const item = await nextPending(slim.id);
            const b = item ? await getBroadcast(slim.id) : slim;
            if (!b || b.status !== "running") continue;
            if (!item) {
              delete queues.current[slim.id];
              await setBroadcastStatus(b.id, "done");
              if (useSettings.getState().notifications) sendNotification({ title: "Broadcast finished", body: b.name ?? b.id });
              qc.invalidateQueries({ queryKey: ["broadcasts"] });
              continue;
            }
            const media = await listBroadcastMedia(slim.id);
            const ctx = {
              name: item.name ?? item.chat_id.split("@")[0],
              phone: /@(c\.us|s\.whatsapp\.net)$/.test(item.chat_id) ? `+${item.chat_id.split("@")[0]}` : "",
            };
            const text = b.text ? expandTemplate(b.text, ctx) : "";
            const state: SendJob["state"] = {};
            // The caption rides on the first attachment only; without attachments it is a plain text message.
            jobs = media.length
              ? media.map((m, i) => ({ b, item, media: m, caption: i === 0 ? text : "", last: i === media.length - 1, state }))
              : [{ b, item, media: null, caption: text, last: true, state }];
            queues.current[slim.id] = jobs;
          }
          const job = jobs.shift()!;
          try {
            if (job.media) {
              const res = await sendMediaOn(
                job.b.account,
                job.item.chat_id,
                { mimetype: job.media.mime ?? "application/octet-stream", name: job.media.name ?? "file", base64: job.media.b64 },
                job.caption,
              );
              if (res.id && !job.state.messageId) job.state.messageId = res.id;
            } else {
              await sendTextOn(job.b.account, job.item.chat_id, job.caption);
            }
          } catch (e) {
            job.state.failed ??= errMsg(e);
          }
          // More attachments for this recipient go out on the following ticks.
          if (!job.last) continue;
          await markItem(job.item.id, job.state.failed ? "error" : "sent", job.state.failed, job.state.messageId);
          if (job.state.failed) {
            const streak = (errorStreak.current[job.b.id] ?? 0) + 1;
            errorStreak.current[job.b.id] = streak;
            if (streak >= MAX_CONSECUTIVE_ERRORS) {
              errorStreak.current[job.b.id] = 0;
              await setBroadcastStatus(job.b.id, "paused");
              if (useSettings.getState().notifications)
                sendNotification({
                  title: "Broadcast paused",
                  body: `${job.b.name ?? job.b.id}: ${streak} recipients failed in a row. Check the account and resume.`,
                });
            }
          } else {
            errorStreak.current[job.b.id] = 0;
          }
          delete queues.current[job.b.id];
          const wait = job.b.delay_min + Math.random() * Math.max(0, job.b.delay_max - job.b.delay_min);
          nextAt.current[job.b.id] = Date.now() + wait * 1000;
          qc.invalidateQueries({ queryKey: ["broadcast-items", job.b.id] });
          qc.invalidateQueries({ queryKey: ["broadcasts"] });
        }
      } catch (e) {
        console.warn("broadcast tick failed", e);
      } finally {
        busy.current = false;
      }
    };
    void tick();
    const t = setInterval(tick, 1000);
    return () => clearInterval(t);
  }, [qc]);
}

export { getBroadcast };
