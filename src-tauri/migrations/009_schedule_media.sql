-- A scheduled message can send several attachments, one row per file.
CREATE TABLE IF NOT EXISTS schedule_media (
  schedule_id TEXT    NOT NULL,
  position    INTEGER NOT NULL,        -- send order
  kind        TEXT    NOT NULL,        -- 'image' | 'video' | 'file'
  mime        TEXT,
  name        TEXT,
  b64         TEXT    NOT NULL,
  PRIMARY KEY (schedule_id, position)
);
CREATE INDEX IF NOT EXISTS idx_schedule_media ON schedule_media (schedule_id, position);

-- Move the single attachment older versions stored on the schedule row into the new table.
INSERT INTO schedule_media (schedule_id, position, kind, mime, name, b64)
SELECT id, 0, kind, media_mime, media_name, media_b64
FROM schedules
WHERE media_b64 IS NOT NULL;
