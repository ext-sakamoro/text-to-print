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
                id         TEXT PRIMARY KEY,
                email      TEXT,
                license_key TEXT,
                tier       TEXT NOT NULL DEFAULT 'Free',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
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
        Ok(())
    }

    pub fn get_or_create_profile(&self, id: &str) -> Result<Tier> {
        self.conn.execute(
            "INSERT OR IGNORE INTO profiles (id) VALUES (?1)",
            [id],
        )?;
        let tier: String = self.conn.query_row(
            "SELECT tier FROM profiles WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;
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
        } = record;
        self.conn.execute(
            "INSERT INTO generations (id, profile_id, prompt, lol_source, sdf_data, quality, status, is_public)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                id,
                profile_id,
                prompt,
                lol_source,
                sdf_data,
                quality,
                status,
                *is_public as i32,
            ],
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
            "SELECT id, prompt, lol_source, quality, status, is_public, created_at
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
        })
        .unwrap();

        let rows = db.list_generations("user1", 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].prompt, "a cute cat");
        assert_eq!(rows[0].lol_source.as_deref(), Some("sphere(1.0)"));
    }
}
