use crate::config;
use rusqlite::{params, Connection, Result};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

/// A single history entry
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub id: i64,
    pub url: String,
    pub title: Option<String>,
    pub visit_count: i32,
    pub last_visit_time: i64,
    pub first_visit_time: i64,
}

impl HistoryEntry {
    /// Parse the URL string into a Url object
    pub fn parse_url(&self) -> Option<Url> {
        Url::parse(&self.url).ok()
    }

    /// Format the last visit time as a human-readable string
    pub fn last_visit_formatted(&self) -> String {
        // Simple formatting - could be enhanced with chrono
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let diff = now - self.last_visit_time;

        if diff < 60 {
            "Just now".to_string()
        } else if diff < 3600 {
            format!("{} minutes ago", diff / 60)
        } else if diff < 86400 {
            format!("{} hours ago", diff / 3600)
        } else if diff < 604800 {
            format!("{} days ago", diff / 86400)
        } else {
            format!("{} weeks ago", diff / 604800)
        }
    }
}

/// SQLite-based history storage
pub struct HistoryDatabase {
    conn: Connection,
}

impl std::fmt::Debug for HistoryDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HistoryDatabase").finish_non_exhaustive()
    }
}

impl HistoryDatabase {
    /// Create or open a history database in the given profile directory
    pub fn new(profile_path: &Path) -> Result<Self> {
        let db_path = profile_path.join(config::HISTORY_DB);
        let conn = Connection::open(&db_path)?;

        // Create tables if they don't exist
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL UNIQUE,
                title TEXT,
                visit_count INTEGER DEFAULT 1,
                last_visit_time INTEGER NOT NULL,
                first_visit_time INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_history_url ON history(url);
            CREATE INDEX IF NOT EXISTS idx_history_last_visit ON history(last_visit_time DESC);
            CREATE INDEX IF NOT EXISTS idx_history_visit_count ON history(visit_count DESC);
            ",
        )?;

        log::info!("History database opened at {:?}", db_path);

        Ok(Self { conn })
    }

    /// Record a page visit
    pub fn record_visit(&self, url: &Url, title: Option<&str>) -> Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let url_str = url.as_str();

        self.conn.execute(
            "INSERT INTO history (url, title, last_visit_time, first_visit_time, visit_count)
             VALUES (?1, ?2, ?3, ?3, 1)
             ON CONFLICT(url) DO UPDATE SET
                 title = COALESCE(?2, title),
                 visit_count = visit_count + 1,
                 last_visit_time = ?3",
            params![url_str, title, now],
        )?;

        log::debug!("Recorded visit to {}", url_str);

        Ok(())
    }

    /// Update the title for a URL (called when page title changes)
    pub fn update_title(&self, url: &Url, title: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE history SET title = ?1 WHERE url = ?2",
            params![title, url.as_str()],
        )?;
        Ok(())
    }

    /// Search history by URL or title
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<HistoryEntry>> {
        let pattern = format!("%{}%", query);

        let mut stmt = self.conn.prepare(
            "SELECT id, url, title, visit_count, last_visit_time, first_visit_time
             FROM history
             WHERE url LIKE ?1 OR title LIKE ?1
             ORDER BY visit_count DESC, last_visit_time DESC
             LIMIT ?2",
        )?;

        let entries = stmt
            .query_map(params![pattern, limit as i64], |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    title: row.get(2)?,
                    visit_count: row.get(3)?,
                    last_visit_time: row.get(4)?,
                    first_visit_time: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(entries)
    }

    /// Get recent history entries
    pub fn get_recent(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, url, title, visit_count, last_visit_time, first_visit_time
             FROM history
             ORDER BY last_visit_time DESC
             LIMIT ?1",
        )?;

        let entries = stmt
            .query_map(params![limit as i64], |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    title: row.get(2)?,
                    visit_count: row.get(3)?,
                    last_visit_time: row.get(4)?,
                    first_visit_time: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(entries)
    }

    /// Get most visited entries
    pub fn get_most_visited(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, url, title, visit_count, last_visit_time, first_visit_time
             FROM history
             ORDER BY visit_count DESC, last_visit_time DESC
             LIMIT ?1",
        )?;

        let entries = stmt
            .query_map(params![limit as i64], |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    title: row.get(2)?,
                    visit_count: row.get(3)?,
                    last_visit_time: row.get(4)?,
                    first_visit_time: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(entries)
    }

    /// Delete a specific history entry
    pub fn delete_entry(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM history WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Delete entries by URL pattern
    pub fn delete_by_url(&self, url: &str) -> Result<usize> {
        let count = self
            .conn
            .execute("DELETE FROM history WHERE url = ?1", params![url])?;
        Ok(count)
    }

    /// Clear all history
    pub fn clear_all(&self) -> Result<()> {
        self.conn.execute("DELETE FROM history", [])?;
        log::info!("Cleared all history");
        Ok(())
    }

    /// Clear history older than a certain time
    pub fn clear_older_than(&self, timestamp: i64) -> Result<usize> {
        let count = self.conn.execute(
            "DELETE FROM history WHERE last_visit_time < ?1",
            params![timestamp],
        )?;
        log::info!("Cleared {} history entries older than {}", count, timestamp);
        Ok(count)
    }

    /// Get the total number of history entries
    pub fn count(&self) -> Result<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))
    }

    /// Check if a URL exists in history
    pub fn url_exists(&self, url: &Url) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM history WHERE url = ?1",
            params![url.as_str()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Get entry by URL
    pub fn get_by_url(&self, url: &Url) -> Result<Option<HistoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, url, title, visit_count, last_visit_time, first_visit_time
             FROM history
             WHERE url = ?1",
        )?;

        let entry = stmt
            .query_row(params![url.as_str()], |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    title: row.get(2)?,
                    visit_count: row.get(3)?,
                    last_visit_time: row.get(4)?,
                    first_visit_time: row.get(5)?,
                })
            })
            .ok();

        Ok(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_record_and_search() {
        let dir = tempdir().unwrap();
        let db = HistoryDatabase::new(dir.path()).unwrap();

        let url = Url::parse("https://example.com").unwrap();
        db.record_visit(&url, Some("Example")).unwrap();

        let results = db.search("example", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].url, "https://example.com/");
        assert_eq!(results[0].title, Some("Example".to_string()));
        assert_eq!(results[0].visit_count, 1);
    }

    #[test]
    fn test_visit_count_increment() {
        let dir = tempdir().unwrap();
        let db = HistoryDatabase::new(dir.path()).unwrap();

        let url = Url::parse("https://example.com").unwrap();
        db.record_visit(&url, Some("Example")).unwrap();
        db.record_visit(&url, None).unwrap();
        db.record_visit(&url, None).unwrap();

        let entry = db.get_by_url(&url).unwrap().unwrap();
        assert_eq!(entry.visit_count, 3);
    }

    #[test]
    fn test_clear_all() {
        let dir = tempdir().unwrap();
        let db = HistoryDatabase::new(dir.path()).unwrap();

        let url1 = Url::parse("https://example1.com").unwrap();
        let url2 = Url::parse("https://example2.com").unwrap();
        db.record_visit(&url1, None).unwrap();
        db.record_visit(&url2, None).unwrap();

        assert_eq!(db.count().unwrap(), 2);

        db.clear_all().unwrap();

        assert_eq!(db.count().unwrap(), 0);
    }
}
