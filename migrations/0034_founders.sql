-- Founding pre-orders linked by email: the plan bought on virling.com and the end of its prepaid first month.
ALTER TABLE accounts ADD COLUMN founder TEXT;
ALTER TABLE accounts ADD COLUMN founder_until INTEGER;
ALTER TABLE billing ADD COLUMN founding INTEGER NOT NULL DEFAULT 0;
