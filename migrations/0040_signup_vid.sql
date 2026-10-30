-- The visitor ID a new account signed up from, carried through its sign-in link.
ALTER TABLE login_tokens ADD COLUMN vid TEXT;
ALTER TABLE accounts ADD COLUMN vid TEXT;
-- Product IDs a payment connection imports, comma separated; NULL imports every product.
ALTER TABLE integrations ADD COLUMN products TEXT;
