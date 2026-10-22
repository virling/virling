-- Signing secret of the LemonSqueezy webhook, encrypted like the API key.
ALTER TABLE integrations ADD COLUMN webhook_ciphertext BLOB;
ALTER TABLE integrations ADD COLUMN webhook_nonce BLOB;
