/**
 * Media downloads run through this queue so that a chat full of auto-loaded photos and
 * videos fetches its attachments a few at a time instead of all at once. Explicit
 * downloads (a click, a save, an AI action) can jump ahead of the auto-load backlog.
 */

/** How many downloads may be in flight together. */
const MAX_CONCURRENT = 3;

let running = 0;
const waiting: (() => void)[] = [];

function pump() {
  while (running < MAX_CONCURRENT && waiting.length > 0) {
    running++;
    waiting.shift()!();
  }
}

/** Run `task` when a download slot is free; `front` puts it ahead of already-queued work. */
export function queueMediaDownload<T>(task: () => Promise<T>, front = false): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const run = () => {
      Promise.resolve()
        .then(task)
        .then(resolve, reject)
        .finally(() => {
          running--;
          pump();
        });
    };
    if (front) waiting.unshift(run);
    else waiting.push(run);
    pump();
  });
}
