-- How many days apart a site's scheduled AI answer runs are.
ALTER TABLE ai_settings ADD COLUMN every_days INTEGER NOT NULL DEFAULT 7;
