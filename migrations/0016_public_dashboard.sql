-- A read-only public dashboard at /share/{domain}, off until the owner turns it on.
ALTER TABLE sites ADD COLUMN public_dashboard INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sites ADD COLUMN public_hide_revenue INTEGER NOT NULL DEFAULT 0;
