ALTER TABLE company_configs
    ADD COLUMN IF NOT EXISTS category TEXT NOT NULL DEFAULT 'config_file',
    ADD COLUMN IF NOT EXISTS file_type TEXT;

-- Backfill existing rows so category is never empty
UPDATE company_configs SET category = 'config_file' WHERE category IS NULL OR category = '';
