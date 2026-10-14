-- First-party analytics for virling.com. Timestamps are unix milliseconds. No IP addresses.
CREATE TABLE events (
  id INTEGER PRIMARY KEY,
  ts INTEGER NOT NULL,
  visitor_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  name TEXT,
  path TEXT NOT NULL,
  referrer_host TEXT,
  utm_source TEXT,
  utm_medium TEXT,
  utm_campaign TEXT,
  utm_content TEXT,
  utm_term TEXT,
  channel TEXT NOT NULL,
  country TEXT,
  device TEXT NOT NULL,
  browser TEXT NOT NULL
);
CREATE INDEX events_ts ON events (ts);
CREATE INDEX events_visitor_ts ON events (visitor_id, ts);

CREATE TABLE conversions (
  id INTEGER PRIMARY KEY,
  ts INTEGER NOT NULL,
  kind TEXT NOT NULL,
  visitor_id TEXT,
  amount_cents INTEGER,
  ref TEXT UNIQUE
);
CREATE INDEX conversions_ts ON conversions (ts);
