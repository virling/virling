-- Addresses and CIDR ranges whose events are dropped at ingest, space-separated.
ALTER TABLE sites ADD COLUMN ignore_ips TEXT NOT NULL DEFAULT '';
