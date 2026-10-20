-- Beta applications: one row per waitlist signup, with the optional second step. Timestamps are unix milliseconds.
CREATE TABLE applications (
  email TEXT NOT NULL,
  wedge TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  site TEXT,
  payments TEXT,
  revenue TEXT,
  audience TEXT,
  PRIMARY KEY (email, wedge)
);
