use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

use crate::tier::Tier;

pub struct GenerationRecord<'a> {
    pub id: &'a str,
    pub profile_id: &'a str,
    pub prompt: &'a str,
    pub lol_source: Option<&'a str>,
    pub sdf_data: Option<&'a [u8]>,
    pub quality: &'a str,
    pub status: &'a str,
    pub is_public: bool,
    /// Optional serialised `AliceManifest` JSON to persist at insert time
    /// May be filled in later via [`Database::set_generation_manifest`]
    pub manifest_json: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct GenerationRow {
    pub id: String,
    pub prompt: String,
    pub lol_source: Option<String>,
    pub quality: Option<String>,
    pub status: String,
    pub is_public: bool,
    pub created_at: String,
    /// Serialised `AliceManifest` JSON (v1 schema, `docs/schema/v1/alice_manifest.schema.json`)
    /// persisted per generation for SharePayload upload (#44) without re-unzipping the 3MF
    pub manifest_json: Option<String>,
}

/// BYO LLM (2026-08-23): a persisted OpenAI-compat provider config The
/// API key itself lives in the OS Keychain and is NOT included here
///
/// See `text_to_print_llm::openai_compat_backend` for the runtime
/// counterpart (`OpenAiCompatConfig`)
#[derive(Debug, Clone)]
pub struct LlmProviderConfigRow {
    /// Stable slug: `"OpenAi"` / `"Anthropic"` / `"Google"` / `"Custom"`
    pub provider: String,
    pub endpoint: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    /// Provider-specific cost-guard: `"minimal"` (OpenAI) / `"none"`
    /// (Gemini) / `None` (Anthropic / Custom) See
    /// `[[llm-api-cost-guard]]` skill and
    /// `OpenAiCompatProvider::default_reasoning_effort` for the rules
    pub reasoning_effort: Option<String>,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS profiles (
                id             TEXT PRIMARY KEY,
                email          TEXT,
                license_key    TEXT,
                tier           TEXT NOT NULL DEFAULT 'Free',
                share_lol_dsl  INTEGER NOT NULL DEFAULT 1,
                created_at     TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at     TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS generations (
                id             TEXT PRIMARY KEY,
                profile_id     TEXT NOT NULL REFERENCES profiles(id),
                prompt         TEXT NOT NULL,
                lol_source     TEXT,
                sdf_data       BLOB,
                triangle_count INTEGER,
                vertex_count   INTEGER,
                quality        TEXT,
                status         TEXT NOT NULL DEFAULT 'pending',
                error          TEXT,
                is_public      INTEGER NOT NULL DEFAULT 0,
                created_at     TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS projects (
                id          TEXT PRIMARY KEY,
                profile_id  TEXT NOT NULL REFERENCES profiles(id),
                name        TEXT NOT NULL,
                description TEXT,
                config      TEXT,
                is_public   INTEGER NOT NULL DEFAULT 0,
                created_at  TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS daily_usage (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                profile_id TEXT NOT NULL REFERENCES profiles(id),
                date       TEXT NOT NULL,
                count      INTEGER NOT NULL DEFAULT 0,
                UNIQUE(profile_id, date)
            );
            ",
        )?;
        // Idempotent addition for pre-existing DBs that predate share_lol_dsl.
        // rusqlite surfaces "duplicate column" as an error which we silently
        // absorb — the flag ends up present either way.
        let _ = self.conn.execute(
            "ALTER TABLE profiles ADD COLUMN share_lol_dsl INTEGER NOT NULL DEFAULT 1",
            [],
        );
        // GAP-7 (#45): manifest_json holds the serialised AliceManifest v1 so
        // the SharePayload uploader (GAP-3 #44) can pull it without re-unzipping
        // the exported 3MF. Idempotent for pre-existing DBs.
        let _ = self
            .conn
            .execute("ALTER TABLE generations ADD COLUMN manifest_json TEXT", []);
        // Stage 3-C.6: `backend_kind` persists the user's inference backend
        // choice (Sidecar HTTP vs Embedded in-process) across app restarts
        //
        // v0.1.0-beta.1 (2026-08-07): default を Sidecar → Embedded に変更
        // sidecar は alice-llm-server binary の別途 install を必要とし、
        // 初回起動で spawn 失敗 UX が壊れていた Embedded は alice-llm を
        // rlib 直リンクなので binary 追加なしで動く (詳細は
        // crates/app/src/state.rs 該当箇所コメント)
        //
        // 注意: 既存 install (backend_kind = 'Sidecar' persist 済) には
        // 影響なし ALTER TABLE ADD COLUMN の DEFAULT は新規行のみ適用
        let _ = self.conn.execute(
            "ALTER TABLE profiles ADD COLUMN backend_kind TEXT NOT NULL DEFAULT 'Embedded'",
            [],
        );
        // Stage 3-C.12: `execution_mode` persists whether the Embedded
        // backend runs on CPU or GPU Default `Cpu` matches pre-3-C.12
        // behaviour and is the always-safe fallback (GPU init can fail
        // if no wgpu adapter is available)
        let _ = self.conn.execute(
            "ALTER TABLE profiles ADD COLUMN execution_mode TEXT NOT NULL DEFAULT 'Cpu'",
            [],
        );
        // Stage 3-C.14: `enforce_lol_grammar` gates the LOL GBNF being
        // sent along with every generation request
        //
        // v0.1.0-beta.1 (2026-08-07): default を `1` → `0` に変更
        // alice-llm-server の grammar constrained decoding が Apple M3
        // iGPU + 3B model でも 20 token / 60s 以上と実用不能 (user 実測)
        // Grammar OFF なら LLM 自由出力 + parse 失敗時 retry loop で救済
        // Users can toggle it back on in Settings when sidecar-side が
        // 高速化されたら on default に戻す
        let _ = self.conn.execute(
            "ALTER TABLE profiles ADD COLUMN enforce_lol_grammar INTEGER NOT NULL DEFAULT 0",
            [],
        );
        // Sprint X.1 (2026-08-21): archetype preset library sync cache
        // Cloudflare Worker `GET /api/presets` の response を local に持つ、
        // offline / 起動時 network 未接続でも last-known preset で app が動く
        // `id = 1` 縛りで single-row 運用 (KV 全体を JSON blob として保存)
        // 詳細: memory/project_text_to_print_archetype_library_architecture.md
        let _ = self.conn.execute(
            "CREATE TABLE IF NOT EXISTS presets_cache (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                version TEXT NOT NULL,
                etag TEXT,
                json_content TEXT NOT NULL,
                fetched_at TEXT NOT NULL
            )",
            [],
        );
        // 2026-08-23 Network settings (user configurable via Settings UI)
        // - presets_endpoint: Cloudflare preset library URL 空文字なら default
        //   (`https://text-to-print.alicelaw.net/api/presets`) 使用、custom URL
        //   入れれば self-hosted mirror や proxy 経由に切替可
        // - presets_sync_enabled: 起動時 background sync の有効化、offline
        //   運用や自 endpoint 固定運用時に 0 で完全 skip 可
        // - sidecar_port: sidecar alice-llm-server の preferred port、他 app
        //   と衝突時に user 側で変更可 (port 使用中なら +1 で自動 fallback)
        let _ = self.conn.execute(
            "ALTER TABLE profiles ADD COLUMN presets_endpoint TEXT NOT NULL DEFAULT ''",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE profiles ADD COLUMN presets_sync_enabled INTEGER NOT NULL DEFAULT 1",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE profiles ADD COLUMN sidecar_port INTEGER NOT NULL DEFAULT 8000",
            [],
        );
        // BYO LLM (2026-08-23): OpenAI-compat provider configs
        //
        // One row per (profile_id, provider) so the user can store
        // credentials for multiple providers (OpenAI + Anthropic +
        // Google + Ollama etc) and switch without re-entering
        //
        // API keys live in the OS Keychain (`crate::keychain`) — this
        // table only holds the non-secret portion of the config
        let _ = self.conn.execute(
            "CREATE TABLE IF NOT EXISTS llm_provider_configs (
                profile_id       TEXT NOT NULL REFERENCES profiles(id),
                provider         TEXT NOT NULL,
                endpoint         TEXT NOT NULL,
                model            TEXT NOT NULL,
                max_tokens       INTEGER NOT NULL DEFAULT 256,
                temperature      REAL NOT NULL DEFAULT 0.7,
                reasoning_effort TEXT,
                created_at       TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (profile_id, provider)
            )",
            [],
        );
        // BYO LLM (2026-08-23): which OpenAI-compat provider is
        // currently active for generation Only meaningful when
        // `backend_kind = 'OpenAiCompat'` Default `OpenAi` chosen to
        // match Settings UI first-preset landing
        let _ = self.conn.execute(
            "ALTER TABLE profiles ADD COLUMN openai_compat_active_provider TEXT NOT NULL DEFAULT 'OpenAi'",
            [],
        );
        // BYO LLM (2026-08-23): user-supplied GGUF file path that
        // overrides the built-in ModelChoice download path when the
        // Embedded backend is active Empty string = not set (use the
        // download path for the current ModelChoice) When populated,
        // spawn_embedded_load skips the HF download and loads directly
        // from this file — user is responsible for placing it on disk
        let _ = self.conn.execute(
            "ALTER TABLE profiles ADD COLUMN custom_gguf_path TEXT NOT NULL DEFAULT ''",
            [],
        );
        // Gallery Phase 1 (2026-08-26): user-visible nickname shown in
        // Gallery in place of the raw DID hex Empty string = not set,
        // Gallery falls back to DID short-form display Max 32 char
        // enforced UI-side (Settings TextEdit)
        let _ = self.conn.execute(
            "ALTER TABLE profiles ADD COLUMN nickname TEXT NOT NULL DEFAULT ''",
            [],
        );
        // Gallery Phase 2 (2026-08-26): per-generation share auto vs
        // confirm-dialog preference `0` = confirm dialog every generation
        // (default for β), `1` = auto share without dialog Only meaningful
        // when the tier is Free and `share_lol_dsl` is on
        let _ = self.conn.execute(
            "ALTER TABLE profiles ADD COLUMN gallery_auto_share INTEGER NOT NULL DEFAULT 0",
            [],
        );
        Ok(())
    }

    pub fn get_or_create_profile(&self, id: &str) -> Result<Tier> {
        self.conn
            .execute("INSERT OR IGNORE INTO profiles (id) VALUES (?1)", [id])?;
        let tier: String =
            self.conn
                .query_row("SELECT tier FROM profiles WHERE id = ?1", [id], |row| {
                    row.get(0)
                })?;
        match tier.as_str() {
            "General" => Ok(Tier::General),
            "Pro" => Ok(Tier::Pro),
            "Enterprise" => Ok(Tier::Enterprise),
            _ => Ok(Tier::Free),
        }
    }

    pub fn get_daily_usage(&self, profile_id: &str, date: &str) -> Result<u32> {
        let count: u32 = self
            .conn
            .query_row(
                "SELECT COALESCE(count, 0) FROM daily_usage WHERE profile_id = ?1 AND date = ?2",
                rusqlite::params![profile_id, date],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok(count)
    }

    pub fn increment_daily_usage(&self, profile_id: &str, date: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO daily_usage (profile_id, date, count) VALUES (?1, ?2, 1)
             ON CONFLICT(profile_id, date) DO UPDATE SET count = count + 1",
            rusqlite::params![profile_id, date],
        )?;
        Ok(())
    }

    pub fn insert_generation(&self, record: &GenerationRecord<'_>) -> Result<()> {
        let GenerationRecord {
            id,
            profile_id,
            prompt,
            lol_source,
            sdf_data,
            quality,
            status,
            is_public,
            manifest_json,
        } = record;
        self.conn.execute(
            "INSERT INTO generations (id, profile_id, prompt, lol_source, sdf_data, quality, status, is_public, manifest_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                id,
                profile_id,
                prompt,
                lol_source,
                sdf_data,
                quality,
                status,
                i32::from(*is_public),
                manifest_json,
            ],
        )?;
        Ok(())
    }

    /// Persist the serialised `AliceManifest` JSON for a completed generation
    ///
    /// Called after `Stage 4` pipeline finishes 3MF export with metadata so
    /// the SharePayload upload path (GAP-3 #44) can retrieve the manifest
    /// without re-parsing the 3MF ZIP
    ///
    /// # Errors
    ///
    /// - SQLite error from `UPDATE`
    pub fn set_generation_manifest(&self, id: &str, manifest_json: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE generations SET manifest_json = ?2 WHERE id = ?1",
            rusqlite::params![id, manifest_json],
        )?;
        Ok(())
    }

    /// Retrieve the persisted manifest JSON for a generation, if any
    ///
    /// # Errors
    ///
    /// - SQLite error from `SELECT`
    pub fn get_generation_manifest(&self, id: &str) -> Result<Option<String>> {
        let manifest: Option<String> = self
            .conn
            .query_row(
                "SELECT manifest_json FROM generations WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap_or(None);
        Ok(manifest)
    }

    /// Fetch the LoRA share opt-in flag for the given profile Defaults to
    /// `true` (share on) when the row is missing so that new profiles
    /// contribute to the shared LoRA training set by default
    pub fn get_share_lol_dsl(&self, profile_id: &str) -> Result<bool> {
        let value: i64 = self
            .conn
            .query_row(
                "SELECT share_lol_dsl FROM profiles WHERE id = ?1",
                [profile_id],
                |row| row.get(0),
            )
            .unwrap_or(1);
        Ok(value != 0)
    }

    /// Update the LoRA share opt-in flag for the given profile
    pub fn set_share_lol_dsl(&self, profile_id: &str, value: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE profiles SET share_lol_dsl = ?2, updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![profile_id, i64::from(value)],
        )?;
        Ok(())
    }

    /// Fetch the user-visible nickname for the given profile Empty string
    /// means unset — callers should fall back to a DID short-form display
    ///
    /// # Errors
    /// - SQLite error from `SELECT`
    pub fn get_nickname(&self, profile_id: &str) -> Result<String> {
        let value: String = self
            .conn
            .query_row(
                "SELECT nickname FROM profiles WHERE id = ?1",
                [profile_id],
                |row| row.get(0),
            )
            .unwrap_or_default();
        Ok(value)
    }

    /// Persist the user-visible nickname Caller is responsible for
    /// trimming and length capping (Settings UI enforces 32 char max)
    ///
    /// # Errors
    /// - SQLite error from `UPDATE`
    pub fn set_nickname(&self, profile_id: &str, nickname: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE profiles SET nickname = ?2, updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![profile_id, nickname],
        )?;
        Ok(())
    }

    /// Fetch the Gallery auto-share preference `false` (default) means
    /// the app pops a confirm dialog after every Free-tier generation;
    /// `true` means auto-publish without dialog
    ///
    /// # Errors
    /// - SQLite error from `SELECT`
    pub fn get_gallery_auto_share(&self, profile_id: &str) -> Result<bool> {
        let value: i64 = self
            .conn
            .query_row(
                "SELECT gallery_auto_share FROM profiles WHERE id = ?1",
                [profile_id],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok(value != 0)
    }

    /// Persist the Gallery auto-share preference
    ///
    /// # Errors
    /// - SQLite error from `UPDATE`
    pub fn set_gallery_auto_share(&self, profile_id: &str, value: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE profiles SET gallery_auto_share = ?2, updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![profile_id, i64::from(value)],
        )?;
        Ok(())
    }

    /// Fetch the persisted `backend_kind` slug for the given profile
    /// Missing / unknown rows fall back to `"Embedded"` (v0.1.0-beta.1
    /// default 変更、alice-llm-server binary 不要で OOB 起動可能な方) 詳細
    /// は migrate() の該当 ALTER TABLE コメント参照
    pub fn get_backend_kind(&self, profile_id: &str) -> Result<String> {
        let value: String = self
            .conn
            .query_row(
                "SELECT backend_kind FROM profiles WHERE id = ?1",
                [profile_id],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| "Embedded".to_string());
        Ok(value)
    }

    /// Persist the user's inference backend choice for the given profile
    pub fn set_backend_kind(&self, profile_id: &str, kind: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE profiles SET backend_kind = ?2, updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![profile_id, kind],
        )?;
        Ok(())
    }

    /// Fetch the persisted `execution_mode` slug for the given profile
    /// Missing / unknown rows fall back to `"Cpu"` so pre-3-C.12 DBs and
    /// systems without a GPU behave sensibly
    pub fn get_execution_mode(&self, profile_id: &str) -> Result<String> {
        let value: String = self
            .conn
            .query_row(
                "SELECT execution_mode FROM profiles WHERE id = ?1",
                [profile_id],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| "Cpu".to_string());
        Ok(value)
    }

    /// Persist the user's Embedded execution mode for the given profile
    pub fn set_execution_mode(&self, profile_id: &str, mode: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE profiles SET execution_mode = ?2, updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![profile_id, mode],
        )?;
        Ok(())
    }

    /// Stage 3-C.14: fetch the LOL GBNF enforcement toggle
    ///
    /// v0.1.0-beta.1 (2026-08-07): default を `true` → `false` に変更
    /// alice-llm-server の grammar constrained decoding が Apple M3 iGPU
    /// で 20 token / 60s 以上と実用不能な遅さ (user 実測)
    /// grammar OFF なら LLM 自由出力 + parse 失敗時 retry loop で救済
    pub fn get_enforce_lol_grammar(&self, profile_id: &str) -> Result<bool> {
        let value: i64 = self
            .conn
            .query_row(
                "SELECT enforce_lol_grammar FROM profiles WHERE id = ?1",
                [profile_id],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok(value != 0)
    }

    /// Persist the LOL GBNF enforcement toggle
    pub fn set_enforce_lol_grammar(&self, profile_id: &str, value: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE profiles SET enforce_lol_grammar = ?2, updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![profile_id, i64::from(value)],
        )?;
        Ok(())
    }

    /// 2026-08-23: Network settings — Cloudflare preset library endpoint
    ///
    /// 空文字なら default endpoint (`PresetsClient::default_endpoint()`) 使用
    /// custom URL 入れれば self-hosted mirror や proxy 経由に切替可
    pub fn get_presets_endpoint(&self, profile_id: &str) -> Result<String> {
        let value: String = self
            .conn
            .query_row(
                "SELECT presets_endpoint FROM profiles WHERE id = ?1",
                [profile_id],
                |row| row.get(0),
            )
            .unwrap_or_default();
        Ok(value)
    }

    pub fn set_presets_endpoint(&self, profile_id: &str, endpoint: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE profiles SET presets_endpoint = ?2, updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![profile_id, endpoint],
        )?;
        Ok(())
    }

    /// 2026-08-23: Network settings — preset library 起動時 background sync 有効化
    ///
    /// `false` で完全 skip (offline 運用 / 自 endpoint 固定 / bundled のみ)
    pub fn get_presets_sync_enabled(&self, profile_id: &str) -> Result<bool> {
        let value: i64 = self
            .conn
            .query_row(
                "SELECT presets_sync_enabled FROM profiles WHERE id = ?1",
                [profile_id],
                |row| row.get(0),
            )
            .unwrap_or(1);
        Ok(value != 0)
    }

    pub fn set_presets_sync_enabled(&self, profile_id: &str, value: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE profiles SET presets_sync_enabled = ?2, updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![profile_id, i64::from(value)],
        )?;
        Ok(())
    }

    /// 2026-08-23: Network settings — sidecar alice-llm-server preferred port
    ///
    /// default 8000 port 使用中の場合は起動時 +1 で自動 fallback (8001)
    /// user 環境で 8000 / 8001 とも別 app に占有される時に override 可
    pub fn get_sidecar_port(&self, profile_id: &str) -> Result<u16> {
        let value: i64 = self
            .conn
            .query_row(
                "SELECT sidecar_port FROM profiles WHERE id = ?1",
                [profile_id],
                |row| row.get(0),
            )
            .unwrap_or(8000);
        Ok(value.clamp(1024, 65535) as u16)
    }

    pub fn set_sidecar_port(&self, profile_id: &str, port: u16) -> Result<()> {
        self.conn.execute(
            "UPDATE profiles SET sidecar_port = ?2, updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![profile_id, i64::from(port)],
        )?;
        Ok(())
    }

    // ── BYO LLM (2026-08-23): OpenAI-compat provider configs ──

    /// Fetch which OpenAI-compat provider is currently active for
    /// generation Only meaningful when `backend_kind = 'OpenAiCompat'`
    /// Missing / unknown rows fall back to `"OpenAi"`
    ///
    /// # Errors
    ///
    /// SQLite query error other than `NoRows` (which is folded into the
    /// default)
    pub fn get_openai_compat_active_provider(&self, profile_id: &str) -> Result<String> {
        let value: String = self
            .conn
            .query_row(
                "SELECT openai_compat_active_provider FROM profiles WHERE id = ?1",
                [profile_id],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| "OpenAi".to_string());
        Ok(value)
    }

    /// Persist which OpenAI-compat provider should route generation
    /// when the backend kind is `OpenAiCompat`
    ///
    /// # Errors
    ///
    /// SQLite UPDATE error
    pub fn set_openai_compat_active_provider(
        &self,
        profile_id: &str,
        provider: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE profiles SET openai_compat_active_provider = ?2, updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![profile_id, provider],
        )?;
        Ok(())
    }

    /// Fetch the stored config for a single (profile, provider) pair
    /// Returns `Ok(None)` when the user has not configured that
    /// provider yet
    ///
    /// # Errors
    ///
    /// SQLite query error other than `NoRows` (which becomes `None`)
    pub fn get_llm_provider_config(
        &self,
        profile_id: &str,
        provider: &str,
    ) -> Result<Option<LlmProviderConfigRow>> {
        let row = self
            .conn
            .query_row(
                "SELECT provider, endpoint, model, max_tokens, temperature, reasoning_effort
                 FROM llm_provider_configs
                 WHERE profile_id = ?1 AND provider = ?2",
                rusqlite::params![profile_id, provider],
                |row| {
                    Ok(LlmProviderConfigRow {
                        provider: row.get(0)?,
                        endpoint: row.get(1)?,
                        model: row.get(2)?,
                        max_tokens: row.get::<_, i64>(3)?.clamp(1, i64::from(u32::MAX)) as u32,
                        temperature: row.get::<_, f64>(4)? as f32,
                        reasoning_effort: row.get(5)?,
                    })
                },
            )
            .ok();
        Ok(row)
    }

    /// List all persisted provider configs for the given profile
    /// Ordered by provider slug for stable UI rendering
    ///
    /// # Errors
    ///
    /// SQLite query / row extraction error
    pub fn list_llm_provider_configs(&self, profile_id: &str) -> Result<Vec<LlmProviderConfigRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT provider, endpoint, model, max_tokens, temperature, reasoning_effort
             FROM llm_provider_configs
             WHERE profile_id = ?1
             ORDER BY provider",
        )?;
        let rows = stmt.query_map([profile_id], |row| {
            Ok(LlmProviderConfigRow {
                provider: row.get(0)?,
                endpoint: row.get(1)?,
                model: row.get(2)?,
                max_tokens: row.get::<_, i64>(3)?.clamp(1, i64::from(u32::MAX)) as u32,
                temperature: row.get::<_, f64>(4)? as f32,
                reasoning_effort: row.get(5)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Upsert a provider config Overwrites the previous row for the
    /// same (profile, provider) pair
    ///
    /// # Errors
    ///
    /// SQLite upsert error
    pub fn set_llm_provider_config(
        &self,
        profile_id: &str,
        config: &LlmProviderConfigRow,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO llm_provider_configs
                (profile_id, provider, endpoint, model, max_tokens, temperature, reasoning_effort, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))
             ON CONFLICT(profile_id, provider) DO UPDATE SET
                endpoint = excluded.endpoint,
                model = excluded.model,
                max_tokens = excluded.max_tokens,
                temperature = excluded.temperature,
                reasoning_effort = excluded.reasoning_effort,
                updated_at = datetime('now')",
            rusqlite::params![
                profile_id,
                config.provider,
                config.endpoint,
                config.model,
                i64::from(config.max_tokens),
                f64::from(config.temperature),
                config.reasoning_effort,
            ],
        )?;
        Ok(())
    }

    /// Remove a stored provider config Idempotent — no error if the row
    /// does not exist Caller is responsible for removing the associated
    /// Keychain entry separately via [`crate::keychain::delete_api_key`]
    ///
    /// # Errors
    ///
    /// SQLite DELETE error
    pub fn delete_llm_provider_config(&self, profile_id: &str, provider: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM llm_provider_configs WHERE profile_id = ?1 AND provider = ?2",
            rusqlite::params![profile_id, provider],
        )?;
        Ok(())
    }

    /// BYO LLM (2026-08-23): user-supplied GGUF path that overrides the
    /// download path when Embedded is active Empty string returned = not
    /// set (use ModelChoice default) Callers convert `""` to `None`
    ///
    /// # Errors
    ///
    /// SQLite query error other than `NoRows` (folded into empty string)
    pub fn get_custom_gguf_path(&self, profile_id: &str) -> Result<String> {
        let value: String = self
            .conn
            .query_row(
                "SELECT custom_gguf_path FROM profiles WHERE id = ?1",
                [profile_id],
                |row| row.get(0),
            )
            .unwrap_or_default();
        Ok(value)
    }

    /// Persist the user-supplied GGUF override path Pass empty string
    /// to clear the override (revert to ModelChoice default)
    ///
    /// # Errors
    ///
    /// SQLite UPDATE error
    pub fn set_custom_gguf_path(&self, profile_id: &str, path: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE profiles SET custom_gguf_path = ?2, updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![profile_id, path],
        )?;
        Ok(())
    }

    /// Sprint X.1: preset cache から (json_content, etag) 読み出し
    ///
    /// 未 cache の場合 (未 sync or DB fresh) は `Ok(None)`、以降 caller で
    /// bundled default にフォールバック
    ///
    /// # Errors
    ///
    /// SQLite query error 以外は None 扱い (No rows は正常)
    pub fn get_presets_cache(&self) -> Result<Option<(String, Option<String>)>> {
        let row: Option<(String, Option<String>)> = self
            .conn
            .query_row(
                "SELECT json_content, etag FROM presets_cache WHERE id = 1",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .ok();
        Ok(row)
    }

    /// Sprint X.1: preset cache 更新 (upsert `id = 1`)
    ///
    /// Cloudflare Worker から fetch 成功時に呼出、次回 startup 時に local から
    /// last-known preset を復元して offline でも動作 (bundled default より優先)
    ///
    /// # Errors
    ///
    /// SQLite upsert error (permission / disk full 等)
    pub fn set_presets_cache(
        &self,
        version: &str,
        etag: Option<&str>,
        json_content: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO presets_cache (id, version, etag, json_content, fetched_at)
             VALUES (1, ?1, ?2, ?3, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
                 version = excluded.version,
                 etag = excluded.etag,
                 json_content = excluded.json_content,
                 fetched_at = excluded.fetched_at",
            rusqlite::params![version, etag, json_content],
        )?;
        Ok(())
    }

    pub fn update_profile_tier(&self, id: &str, tier: &str, license_key: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE profiles SET tier = ?2, license_key = ?3, updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![id, tier, license_key],
        )?;
        Ok(())
    }

    pub fn update_generation_status(
        &self,
        id: &str,
        status: &str,
        lol_source: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE generations SET status = ?2, lol_source = COALESCE(?3, lol_source), error = ?4 WHERE id = ?1",
            rusqlite::params![id, status, lol_source, error],
        )?;
        Ok(())
    }

    pub fn list_generations(&self, profile_id: &str, limit: u32) -> Result<Vec<GenerationRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, prompt, lol_source, quality, status, is_public, created_at, manifest_json
             FROM generations WHERE profile_id = ?1
             ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![profile_id, limit], |row| {
            Ok(GenerationRow {
                id: row.get(0)?,
                prompt: row.get(1)?,
                lol_source: row.get(2)?,
                quality: row.get(3)?,
                status: row.get(4)?,
                is_public: row.get::<_, i32>(5)? != 0,
                created_at: row.get(6)?,
                manifest_json: row.get(7)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_db() -> Database {
        Database::open(&PathBuf::from(":memory:")).unwrap()
    }

    #[test]
    fn test_profile_and_usage() {
        let db = test_db();
        let tier = db.get_or_create_profile("user1").unwrap();
        assert_eq!(tier, Tier::Free);

        let usage = db.get_daily_usage("user1", "2026-04-13").unwrap();
        assert_eq!(usage, 0);

        db.increment_daily_usage("user1", "2026-04-13").unwrap();
        let usage = db.get_daily_usage("user1", "2026-04-13").unwrap();
        assert_eq!(usage, 1);
    }

    #[test]
    fn share_lol_dsl_defaults_to_true() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        assert!(db.get_share_lol_dsl("user1").unwrap());
    }

    #[test]
    fn share_lol_dsl_roundtrip() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        db.set_share_lol_dsl("user1", false).unwrap();
        assert!(!db.get_share_lol_dsl("user1").unwrap());
        db.set_share_lol_dsl("user1", true).unwrap();
        assert!(db.get_share_lol_dsl("user1").unwrap());
    }

    #[test]
    fn share_lol_dsl_unknown_profile_returns_true() {
        let db = test_db();
        assert!(db.get_share_lol_dsl("nonexistent").unwrap());
    }

    #[test]
    fn nickname_defaults_to_empty() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        assert_eq!(db.get_nickname("user1").unwrap(), "");
    }

    #[test]
    fn nickname_roundtrip() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        db.set_nickname("user1", "alice").unwrap();
        assert_eq!(db.get_nickname("user1").unwrap(), "alice");
        db.set_nickname("user1", "アリス").unwrap();
        assert_eq!(db.get_nickname("user1").unwrap(), "アリス");
    }

    #[test]
    fn nickname_unknown_profile_returns_empty() {
        let db = test_db();
        assert_eq!(db.get_nickname("nonexistent").unwrap(), "");
    }

    #[test]
    fn gallery_auto_share_defaults_to_false() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        assert!(!db.get_gallery_auto_share("user1").unwrap());
    }

    #[test]
    fn gallery_auto_share_roundtrip() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        db.set_gallery_auto_share("user1", true).unwrap();
        assert!(db.get_gallery_auto_share("user1").unwrap());
        db.set_gallery_auto_share("user1", false).unwrap();
        assert!(!db.get_gallery_auto_share("user1").unwrap());
    }

    #[test]
    fn backend_kind_defaults_to_embedded() {
        // v0.1.0-beta.1 (2026-08-07): default を Sidecar → Embedded に変更
        // alice-llm-server binary 別途 install 不要の OOB 起動 UX 優先
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        assert_eq!(db.get_backend_kind("user1").unwrap(), "Embedded");
    }

    #[test]
    fn backend_kind_roundtrip() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        db.set_backend_kind("user1", "Embedded").unwrap();
        assert_eq!(db.get_backend_kind("user1").unwrap(), "Embedded");
        db.set_backend_kind("user1", "Sidecar").unwrap();
        assert_eq!(db.get_backend_kind("user1").unwrap(), "Sidecar");
    }

    #[test]
    fn backend_kind_unknown_profile_falls_back_to_embedded() {
        // v0.1.0-beta.1: default 変更に追随、`get_backend_kind` の
        // `unwrap_or_else` fallback も Embedded に揃えた
        let db = test_db();
        assert_eq!(db.get_backend_kind("nonexistent").unwrap(), "Embedded");
    }

    #[test]
    fn execution_mode_defaults_to_cpu() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        assert_eq!(db.get_execution_mode("user1").unwrap(), "Cpu");
    }

    #[test]
    fn execution_mode_roundtrip() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        db.set_execution_mode("user1", "Gpu").unwrap();
        assert_eq!(db.get_execution_mode("user1").unwrap(), "Gpu");
        db.set_execution_mode("user1", "Cpu").unwrap();
        assert_eq!(db.get_execution_mode("user1").unwrap(), "Cpu");
    }

    #[test]
    fn execution_mode_unknown_profile_falls_back_to_cpu() {
        let db = test_db();
        assert_eq!(db.get_execution_mode("nonexistent").unwrap(), "Cpu");
    }

    #[test]
    fn enforce_lol_grammar_defaults_to_false() {
        // v0.1.0-beta.1 (2026-08-07): default を true → false に変更
        // (alice-llm-server grammar constrained decoding が iGPU で実用不能な
        // 遅さのため、free-form + retry loop 救済で切替)
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        assert!(!db.get_enforce_lol_grammar("user1").unwrap());
    }

    #[test]
    fn enforce_lol_grammar_roundtrip() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        db.set_enforce_lol_grammar("user1", false).unwrap();
        assert!(!db.get_enforce_lol_grammar("user1").unwrap());
        db.set_enforce_lol_grammar("user1", true).unwrap();
        assert!(db.get_enforce_lol_grammar("user1").unwrap());
    }

    #[test]
    fn enforce_lol_grammar_unknown_profile_returns_false() {
        // v0.1.0-beta.1: default 変更に追随
        let db = test_db();
        assert!(!db.get_enforce_lol_grammar("nonexistent").unwrap());
    }

    // ── Sprint X.1: presets_cache tests ──

    #[test]
    fn presets_cache_returns_none_when_empty() {
        let db = test_db();
        assert!(db.get_presets_cache().unwrap().is_none());
    }

    #[test]
    fn presets_cache_roundtrip() {
        let db = test_db();
        db.set_presets_cache("v1", Some("etag123"), "{\"version\":\"v1\"}")
            .unwrap();
        let (json, etag) = db.get_presets_cache().unwrap().unwrap();
        assert_eq!(json, "{\"version\":\"v1\"}");
        assert_eq!(etag.as_deref(), Some("etag123"));
    }

    #[test]
    fn presets_cache_upsert_single_row() {
        // id = 1 縛りで single-row 運用、2 回 set しても row は 1 つ、最新 value を保持
        let db = test_db();
        db.set_presets_cache("v1", Some("etag1"), "{\"v\":1}")
            .unwrap();
        db.set_presets_cache("v2", Some("etag2"), "{\"v\":2}")
            .unwrap();
        let (json, etag) = db.get_presets_cache().unwrap().unwrap();
        assert_eq!(json, "{\"v\":2}");
        assert_eq!(etag.as_deref(), Some("etag2"));

        // Verify only 1 row exists (single-row invariant)
        let count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM presets_cache", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn presets_cache_etag_can_be_null() {
        let db = test_db();
        db.set_presets_cache("v1", None, "{}").unwrap();
        let (_json, etag) = db.get_presets_cache().unwrap().unwrap();
        assert!(etag.is_none());
    }

    #[test]
    fn test_insert_and_list_generations() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();

        db.insert_generation(&GenerationRecord {
            id: "gen1",
            profile_id: "user1",
            prompt: "a cute cat",
            lol_source: Some("sphere(1.0)"),
            sdf_data: None,
            quality: "preview",
            status: "complete",
            is_public: false,
            manifest_json: None,
        })
        .unwrap();

        let rows = db.list_generations("user1", 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].prompt, "a cute cat");
        assert_eq!(rows[0].lol_source.as_deref(), Some("sphere(1.0)"));
        assert!(rows[0].manifest_json.is_none());
    }

    #[test]
    fn generation_manifest_roundtrip() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        db.insert_generation(&GenerationRecord {
            id: "gen-with-manifest",
            profile_id: "user1",
            prompt: "cube 10mm",
            lol_source: Some("cube(10.0)"),
            sdf_data: None,
            quality: "high",
            status: "complete",
            is_public: false,
            manifest_json: Some(r#"{"schema_version":"1","uuid":"abc"}"#),
        })
        .unwrap();

        let m = db.get_generation_manifest("gen-with-manifest").unwrap();
        assert_eq!(m.as_deref(), Some(r#"{"schema_version":"1","uuid":"abc"}"#));

        // Update via set_generation_manifest
        db.set_generation_manifest(
            "gen-with-manifest",
            r#"{"schema_version":"1","uuid":"def"}"#,
        )
        .unwrap();
        let m2 = db.get_generation_manifest("gen-with-manifest").unwrap();
        assert_eq!(
            m2.as_deref(),
            Some(r#"{"schema_version":"1","uuid":"def"}"#)
        );

        let rows = db.list_generations("user1", 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].manifest_json.as_deref(),
            Some(r#"{"schema_version":"1","uuid":"def"}"#)
        );
    }

    // ── BYO LLM (2026-08-23): OpenAI-compat provider config tests ──

    #[test]
    fn openai_compat_active_provider_defaults_to_openai() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        assert_eq!(
            db.get_openai_compat_active_provider("user1").unwrap(),
            "OpenAi"
        );
    }

    #[test]
    fn openai_compat_active_provider_roundtrip() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        db.set_openai_compat_active_provider("user1", "Anthropic")
            .unwrap();
        assert_eq!(
            db.get_openai_compat_active_provider("user1").unwrap(),
            "Anthropic"
        );
        db.set_openai_compat_active_provider("user1", "Google")
            .unwrap();
        assert_eq!(
            db.get_openai_compat_active_provider("user1").unwrap(),
            "Google"
        );
    }

    #[test]
    fn openai_compat_active_provider_unknown_profile_defaults_to_openai() {
        let db = test_db();
        assert_eq!(
            db.get_openai_compat_active_provider("nonexistent").unwrap(),
            "OpenAi"
        );
    }

    #[test]
    fn llm_provider_config_get_returns_none_when_missing() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        assert!(
            db.get_llm_provider_config("user1", "OpenAi")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn llm_provider_config_roundtrip() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        let cfg = LlmProviderConfigRow {
            provider: "OpenAi".to_string(),
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            model: "gpt-5".to_string(),
            max_tokens: 512,
            temperature: 0.5,
            reasoning_effort: Some("minimal".to_string()),
        };
        db.set_llm_provider_config("user1", &cfg).unwrap();

        let got = db.get_llm_provider_config("user1", "OpenAi").unwrap();
        let row = got.expect("just inserted");
        assert_eq!(row.provider, "OpenAi");
        assert_eq!(row.endpoint, cfg.endpoint);
        assert_eq!(row.model, cfg.model);
        assert_eq!(row.max_tokens, 512);
        assert!((row.temperature - 0.5).abs() < 1e-6);
        assert_eq!(row.reasoning_effort.as_deref(), Some("minimal"));
    }

    #[test]
    fn llm_provider_config_upsert_overwrites_previous_row() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        let cfg1 = LlmProviderConfigRow {
            provider: "Anthropic".to_string(),
            endpoint: "https://api.anthropic.com/v1/chat/completions".to_string(),
            model: "claude-sonnet-4-5".to_string(),
            max_tokens: 256,
            temperature: 0.7,
            reasoning_effort: None,
        };
        db.set_llm_provider_config("user1", &cfg1).unwrap();

        let cfg2 = LlmProviderConfigRow {
            provider: "Anthropic".to_string(),
            endpoint: "https://api.anthropic.com/v1/chat/completions".to_string(),
            model: "claude-opus-4-7".to_string(),
            max_tokens: 1024,
            temperature: 0.3,
            reasoning_effort: None,
        };
        db.set_llm_provider_config("user1", &cfg2).unwrap();

        let got = db.get_llm_provider_config("user1", "Anthropic").unwrap();
        let row = got.expect("upsert should keep row");
        assert_eq!(row.model, "claude-opus-4-7");
        assert_eq!(row.max_tokens, 1024);

        // Only one row per (profile, provider)
        let all = db.list_llm_provider_configs("user1").unwrap();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn llm_provider_config_list_ordered_by_provider() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        for provider in ["OpenAi", "Google", "Anthropic", "Custom"] {
            db.set_llm_provider_config(
                "user1",
                &LlmProviderConfigRow {
                    provider: provider.to_string(),
                    endpoint: format!("https://{provider}.example/v1/chat/completions"),
                    model: format!("{provider}-model"),
                    max_tokens: 256,
                    temperature: 0.7,
                    reasoning_effort: None,
                },
            )
            .unwrap();
        }
        let all = db.list_llm_provider_configs("user1").unwrap();
        let slugs: Vec<&str> = all.iter().map(|r| r.provider.as_str()).collect();
        assert_eq!(slugs, vec!["Anthropic", "Custom", "Google", "OpenAi"]);
    }

    #[test]
    fn llm_provider_config_delete_is_idempotent() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        // Delete when nothing exists — no error
        db.delete_llm_provider_config("user1", "OpenAi").unwrap();

        db.set_llm_provider_config(
            "user1",
            &LlmProviderConfigRow {
                provider: "OpenAi".to_string(),
                endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
                model: "gpt-5".to_string(),
                max_tokens: 256,
                temperature: 0.7,
                reasoning_effort: Some("minimal".to_string()),
            },
        )
        .unwrap();
        assert!(
            db.get_llm_provider_config("user1", "OpenAi")
                .unwrap()
                .is_some()
        );

        db.delete_llm_provider_config("user1", "OpenAi").unwrap();
        assert!(
            db.get_llm_provider_config("user1", "OpenAi")
                .unwrap()
                .is_none()
        );

        // Delete again — still no error
        db.delete_llm_provider_config("user1", "OpenAi").unwrap();
    }

    #[test]
    fn custom_gguf_path_defaults_to_empty() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        assert_eq!(db.get_custom_gguf_path("user1").unwrap(), "");
    }

    #[test]
    fn custom_gguf_path_roundtrip() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        db.set_custom_gguf_path("user1", "/tmp/qwen2.5-14b.gguf")
            .unwrap();
        assert_eq!(
            db.get_custom_gguf_path("user1").unwrap(),
            "/tmp/qwen2.5-14b.gguf"
        );
        // Clear via empty string
        db.set_custom_gguf_path("user1", "").unwrap();
        assert_eq!(db.get_custom_gguf_path("user1").unwrap(), "");
    }

    #[test]
    fn custom_gguf_path_unknown_profile_returns_empty() {
        let db = test_db();
        assert_eq!(db.get_custom_gguf_path("nonexistent").unwrap(), "");
    }

    #[test]
    fn llm_provider_configs_are_per_profile() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        db.get_or_create_profile("user2").unwrap();

        db.set_llm_provider_config(
            "user1",
            &LlmProviderConfigRow {
                provider: "OpenAi".to_string(),
                endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
                model: "gpt-5".to_string(),
                max_tokens: 256,
                temperature: 0.7,
                reasoning_effort: Some("minimal".to_string()),
            },
        )
        .unwrap();

        assert_eq!(db.list_llm_provider_configs("user1").unwrap().len(), 1);
        assert_eq!(db.list_llm_provider_configs("user2").unwrap().len(), 0);
    }

    #[test]
    fn migration_is_idempotent_across_reopen() {
        // First open creates the columns
        let path = std::env::temp_dir().join(format!(
            "text-to-print-db-idempotent-{}.sqlite",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        {
            let db = Database::open(&path).unwrap();
            db.get_or_create_profile("user1").unwrap();
        }

        // Reopen — migrate() runs again, ALTER TABLE for manifest_json must be
        // absorbed silently even though the column already exists
        {
            let db = Database::open(&path).unwrap();
            // manifest_json accessible = migration succeeded on reopen
            db.insert_generation(&GenerationRecord {
                id: "reopen",
                profile_id: "user1",
                prompt: "x",
                lol_source: None,
                sdf_data: None,
                quality: "preview",
                status: "complete",
                is_public: false,
                manifest_json: Some("{}"),
            })
            .unwrap();
            assert_eq!(
                db.get_generation_manifest("reopen").unwrap().as_deref(),
                Some("{}")
            );
        }

        let _ = std::fs::remove_file(&path);
    }
}
