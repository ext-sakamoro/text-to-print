-- One licence per payment: a licence is stored under an issuance key
-- (`checkout:<session id>` or `subscription:<id>:<period end>`), and a second
-- attempt with the same key keeps the first row (the webhook resends it)
--
-- Apply via:
--   wrangler d1 execute text-to-print-shares --file=migrations/0005_license_issue_key.sql

ALTER TABLE licenses ADD COLUMN issue_key TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_licenses_issue_key ON licenses(issue_key) WHERE issue_key IS NOT NULL;
