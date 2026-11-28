-- Founder numbers, assigned in one statement so two payments at once cannot share a number. plan is the plan's KV count key.
CREATE TABLE founder_numbers (
  plan TEXT NOT NULL,
  payment_id TEXT NOT NULL,
  number INTEGER NOT NULL,
  PRIMARY KEY (plan, payment_id),
  UNIQUE (plan, number)
);
