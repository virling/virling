-- Other domains that belong to a visit, space-separated: clicks to them are not exits.
ALTER TABLE sites ADD COLUMN same_visit TEXT NOT NULL DEFAULT '';
