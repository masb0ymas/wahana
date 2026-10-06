-- How often and when a quick reply was last used, so the composer's template panel can
-- list the ones in use first.
ALTER TABLE quick_replies ADD COLUMN uses INTEGER NOT NULL DEFAULT 0;
ALTER TABLE quick_replies ADD COLUMN last_used INTEGER;
