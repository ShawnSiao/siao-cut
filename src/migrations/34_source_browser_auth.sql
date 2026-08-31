ALTER TABLE source_imports
ADD COLUMN auth_mode TEXT NOT NULL DEFAULT 'anonymous';

ALTER TABLE source_imports
ADD COLUMN browser TEXT;
