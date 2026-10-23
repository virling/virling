ALTER TABLE login_tokens ADD COLUMN ip TEXT;
CREATE INDEX login_tokens_ip ON login_tokens (ip, created_at);
