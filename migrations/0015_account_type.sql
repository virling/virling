-- Personal accounts get the weekly report as an email to themselves; agency accounts get white-label client reports.
ALTER TABLE accounts ADD COLUMN account_type TEXT NOT NULL DEFAULT 'personal';
-- Accounts already using the client report keep it.
UPDATE accounts SET account_type = 'agency' WHERE id IN (
    SELECT s.account_id FROM sites s JOIN report_brands b ON b.site_id = s.id
    WHERE b.agency_name <> '' OR b.logo_url <> '' OR b.accent <> '' OR b.footer <> ''
        OR b.note <> '' OR b.recipients <> '' OR b.share_token IS NOT NULL
);
