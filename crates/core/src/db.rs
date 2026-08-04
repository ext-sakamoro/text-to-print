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
        // Default `Sidecar` matches pre-3-C.6 behaviour.
        let _ = self.conn.execute(
            "ALTER TABLE profiles ADD COLUMN backend_kind TEXT NOT NULL DEFAULT 'Sidecar'",
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
        // sent along with every generation request Default `1` (on) so
        // out-of-the-box output is guaranteed to be parseable Users can
        // toggle it off in Settings when debugging free-form output
        let _ = self.conn.execute(
            "ALTER TABLE profiles ADD COLUMN enforce_lol_grammar INTEGER NOT NULL DEFAULT 1",
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

    /// Fetch the persisted `backend_kind` slug for the given profile
    /// Missing / unknown rows fall back to `"Sidecar"` so pre-3-C.6 DBs
    /// read as the pre-existing behaviour
    pub fn get_backend_kind(&self, profile_id: &str) -> Result<String> {
        let value: String = self
            .conn
            .query_row(
                "SELECT backend_kind FROM profiles WHERE id = ?1",
                [profile_id],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| "Sidecar".to_string());
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

    /// Stage 3-C.14: fetch the LOL GBNF enforcement toggle Missing rows
    /// return `true` so new profiles get grammar-constrained output by
    /// default (the LOL DSL grammar is what alice-lol expects downstream)
    pub fn get_enforce_lol_grammar(&self, profile_id: &str) -> Result<bool> {
        let value: i64 = self
            .conn
            .query_row(
                "SELECT enforce_lol_grammar FROM profiles WHERE id = ?1",
                [profile_id],
                |row| row.get(0),
            )
            .unwrap_or(1);
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
    fn backend_kind_defaults_to_sidecar() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        assert_eq!(db.get_backend_kind("user1").unwrap(), "Sidecar");
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
    fn backend_kind_unknown_profile_falls_back_to_sidecar() {
        let db = test_db();
        assert_eq!(db.get_backend_kind("nonexistent").unwrap(), "Sidecar");
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
    fn enforce_lol_grammar_defaults_to_true() {
        let db = test_db();
        db.get_or_create_profile("user1").unwrap();
        assert!(db.get_enforce_lol_grammar("user1").unwrap());
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
    fn enforce_lol_grammar_unknown_profile_returns_true() {
        let db = test_db();
        assert!(db.get_enforce_lol_grammar("nonexistent").unwrap());
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
