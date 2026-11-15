//! Sites that needed the user: the last wall each showed, and whether the user signed in or
//! passed its check in the browser. A row appears when a site first needs the user or first
//! gets a rule.

use rusqlite::Row;

use crate::{Library, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteRow {
    pub host: String,
    /// A sign-in completed for this site.
    pub signed_in_at: Option<i64>,
    /// A "checking your browser" page was passed.
    pub check_passed_at: Option<i64>,
    /// The last reason it needed the user, by name.
    pub last_wall: Option<String>,
    pub last_wall_at: Option<i64>,
}

fn site(row: &Row) -> rusqlite::Result<SiteRow> {
    Ok(SiteRow {
        host: row.get(0)?,
        signed_in_at: row.get(1)?,
        check_passed_at: row.get(2)?,
        last_wall: row.get(3)?,
        last_wall_at: row.get(4)?,
    })
}

impl Library {
    /// Every site with a row, by host.
    pub fn sites(&self) -> Result<Vec<SiteRow>> {
        let mut statement = self.conn.prepare(
            "SELECT host, signed_in_at, check_passed_at, last_wall, last_wall_at
             FROM sites ORDER BY host",
        )?;
        let rows = statement.query_map([], site)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Makes sure `host` has a row.
    pub fn add_site(&self, host: &str) -> Result<()> {
        self.conn
            .execute("INSERT OR IGNORE INTO sites (host) VALUES (?1)", [host])?;
        Ok(())
    }

    /// `host` needed the user for `wall` at `at`.
    pub fn site_wall(&self, host: &str, wall: &str, at: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sites (host, last_wall, last_wall_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(host) DO UPDATE SET last_wall = ?2, last_wall_at = ?3",
            rusqlite::params![host, wall, at],
        )?;
        Ok(())
    }

    /// The user signed in to `host` at `at`.
    pub fn site_signed_in(&self, host: &str, at: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sites (host, signed_in_at) VALUES (?1, ?2)
             ON CONFLICT(host) DO UPDATE SET signed_in_at = ?2",
            rusqlite::params![host, at],
        )?;
        Ok(())
    }

    /// The user passed `host`'s check at `at`.
    pub fn site_check_passed(&self, host: &str, at: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sites (host, check_passed_at) VALUES (?1, ?2)
             ON CONFLICT(host) DO UPDATE SET check_passed_at = ?2",
            rusqlite::params![host, at],
        )?;
        Ok(())
    }

    /// Forgets that the user signed in to `host` or passed its check (its data was cleared).
    pub fn sign_out_site(&self, host: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sites SET signed_in_at = NULL, check_passed_at = NULL WHERE host = ?1",
            [host],
        )?;
        Ok(())
    }

    /// Forgets every sign-in and passed check (all browser data was removed).
    pub fn sign_out_sites(&self) -> Result<()> {
        self.conn.execute(
            "UPDATE sites SET signed_in_at = NULL, check_passed_at = NULL",
            [],
        )?;
        Ok(())
    }

    /// Drops `host`'s row.
    pub fn forget_site(&self, host: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM sites WHERE host = ?1", [host])?;
        Ok(())
    }
}
