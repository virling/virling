ALTER TABLE waitlist ADD COLUMN leave_token TEXT;
CREATE UNIQUE INDEX waitlist_leave_token ON waitlist (leave_token);
