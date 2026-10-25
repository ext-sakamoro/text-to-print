-- The gallery listing is ordered by the time the worker received each post
-- (received_at), not by the author's created_at
--
-- Apply via:
--   wrangler d1 execute text-to-print-gallery --file=migrations/0004_gallery_received_at.sql

CREATE INDEX IF NOT EXISTS idx_gallery_received_at ON gallery_sdfs(received_at DESC, id DESC);
