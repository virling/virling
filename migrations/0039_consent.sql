-- Cookieless until the consent tool calls virling("consent").
ALTER TABLE sites ADD COLUMN consent INTEGER NOT NULL DEFAULT 0;
