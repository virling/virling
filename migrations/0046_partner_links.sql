-- A partner link carries a token for its read-only report; plain tracked links have none.
ALTER TABLE short_links ADD COLUMN share_token TEXT;
CREATE UNIQUE INDEX short_links_share_token ON short_links (share_token);
