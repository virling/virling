-- The local day the daily digest and the traffic spike message were last sent, as a Julian day.
ALTER TABLE telegram_alerts ADD COLUMN digest_day INTEGER NOT NULL DEFAULT 0;
ALTER TABLE telegram_alerts ADD COLUMN spike_day INTEGER NOT NULL DEFAULT 0;
