-- A Slack incoming webhook the weekly report is also posted to, stored encrypted.
ALTER TABLE report_brands ADD COLUMN slack_ciphertext BLOB;
ALTER TABLE report_brands ADD COLUMN slack_nonce BLOB;
