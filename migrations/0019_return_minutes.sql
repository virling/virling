-- Minutes away after which a visitor's return starts a new visit.
ALTER TABLE sites ADD COLUMN return_minutes INTEGER NOT NULL DEFAULT 5;
