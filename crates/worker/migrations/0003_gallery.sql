-- text-to-print Gallery Phase 3 (2026-08-26): D1 schema for the
-- Cloudflare-relayed Gallery (list / publish / delete)
--
-- Apply via:
--   wrangler d1 create text-to-print-gallery   # get database_id
--   # (paste id into wrangler.toml GALLERY_DB binding)
--   wrangler d1 execute text-to-print-gallery --file=migrations/0003_gallery.sql

CREATE TABLE IF NOT EXISTS gallery_sdfs (
    id              TEXT PRIMARY KEY,           -- UUID v7 from client
    author_did      TEXT NOT NULL,              -- did:key:<64hex ed25519 pub>
    author_nickname TEXT,                       -- display-only, may be NULL
    lol_source      TEXT NOT NULL,              -- LOL DSL (max 100 KB, gated at handler)
    created_at      TEXT NOT NULL,              -- ISO-8601 UTC, from client
    signature       TEXT NOT NULL,              -- hex ed25519 sig over canonical msg
    received_at     TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at      TEXT                        -- soft-delete: NULL = live, set = removed
);

-- Latest-first listing is the common path (Gallery UI list) Filter by
-- `deleted_at IS NULL` inside the handler; index still covers because
-- WHERE + ORDER BY DESC can walk this index backwards
CREATE INDEX IF NOT EXISTS idx_gallery_created_at ON gallery_sdfs(created_at DESC);

-- Own-post lookup (DELETE /api/gallery/:id verifies sig then targets by id)
-- id is already PK so no additional index needed
