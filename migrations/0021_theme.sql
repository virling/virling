-- Each user's color theme: auto follows the system, or light or dark.
ALTER TABLE accounts ADD COLUMN theme TEXT NOT NULL DEFAULT 'auto';
