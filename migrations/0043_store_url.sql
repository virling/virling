-- The store a payment connection reads from, for providers that live on the merchant's own site.
ALTER TABLE integrations ADD COLUMN store_url TEXT;
