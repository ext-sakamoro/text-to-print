-- text-to-print LoRA share upload backend — initial D1 schema
-- Apply via: wrangler d1 execute text-to-print-shares --file=migrations/0001_init.sql

CREATE TABLE IF NOT EXISTS shares (
    uuid                    TEXT PRIMARY KEY,
    -- generation metadata (v1 alice_manifest.schema.json)
    timestamp               TEXT NOT NULL,
    prompt                  TEXT NOT NULL,
    prompt_lang             TEXT NOT NULL,
    llm_model               TEXT NOT NULL,
    lol_source              TEXT NOT NULL,
    lol_sha256              TEXT NOT NULL,
    mesh_sha256             TEXT NOT NULL,
    -- quality signals (piggyback per Analytics 方針 A+B+C)
    success                 INTEGER NOT NULL,
    retry_count             INTEGER NOT NULL,
    time_to_file_ms         INTEGER NOT NULL,
    safety_violations_json  TEXT NOT NULL,
    export_format           TEXT NOT NULL,
    user_kept               INTEGER NOT NULL,
    user_edited             INTEGER NOT NULL,
    -- server-side (privacy: SHA256(ip + IP_HASH_SALT), raw IP never stored)
    ip_hash                 TEXT,
    received_at             TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_shares_received_at ON shares(received_at);
CREATE INDEX IF NOT EXISTS idx_shares_llm_model ON shares(llm_model);

-- Rate limit bookkeeping: rolling per-key counters
CREATE TABLE IF NOT EXISTS rate_limit_counters (
    key         TEXT PRIMARY KEY,   -- e.g. "uuid:<uuid>" or "ip:<hash>"
    count       INTEGER NOT NULL,
    window_ends TEXT NOT NULL       -- ISO-8601 UTC, count resets at this instant
);

CREATE INDEX IF NOT EXISTS idx_rate_limit_window_ends ON rate_limit_counters(window_ends);
