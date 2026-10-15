ALTER TABLE report_brands ADD COLUMN share_token TEXT;
CREATE UNIQUE INDEX report_brands_share_token ON report_brands (share_token);
