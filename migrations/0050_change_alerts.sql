-- Email when revenue, a channel or tracking moves away from usual, with the last day checked as a Julian day.
ALTER TABLE email_alerts ADD COLUMN changes INTEGER NOT NULL DEFAULT 0;
ALTER TABLE email_alerts ADD COLUMN changes_day INTEGER NOT NULL DEFAULT 0;
