-- A broadcast can send several attachments to each recipient, one row per file.
CREATE TABLE IF NOT EXISTS broadcast_media (
  broadcast_id TEXT    NOT NULL,
  position     INTEGER NOT NULL,        -- send order
  kind         TEXT    NOT NULL,        -- 'image' | 'video' | 'file'
  mime         TEXT,
  name         TEXT,
  b64          TEXT    NOT NULL,
  PRIMARY KEY (broadcast_id, position)
);
CREATE INDEX IF NOT EXISTS idx_broadcast_media ON broadcast_media (broadcast_id, position);

-- Move the single attachment older versions stored on the broadcast row into the new table.
INSERT INTO broadcast_media (broadcast_id, position, kind, mime, name, b64)
SELECT id, 0, kind, media_mime, media_name, media_b64
FROM broadcasts
WHERE media_b64 IS NOT NULL;
