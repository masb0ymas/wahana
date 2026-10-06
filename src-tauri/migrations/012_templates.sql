-- Message templates: named messages picked from the composer's "+" menu, with {{name}} and
-- {{phone}} filled in for the chat. Kept apart from quick replies, which are typed as /shortcut,
-- so the usage columns migration 011 gave quick replies are dropped again.
CREATE TABLE IF NOT EXISTS templates (
  id         TEXT PRIMARY KEY,
  account    TEXT NOT NULL,
  name       TEXT NOT NULL,
  text       TEXT NOT NULL,
  uses       INTEGER NOT NULL DEFAULT 0,
  last_used  INTEGER,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_templates_account ON templates (account);
ALTER TABLE quick_replies DROP COLUMN uses;
ALTER TABLE quick_replies DROP COLUMN last_used;
