use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use time::Timespec;

use crate::common::YoutubeID;
use crate::youtube::VideoInfo;

#[derive(Debug)]
struct Person {
    id: i32,
    name: String,
    time_created: Timespec,
    data: Option<Vec<u8>>,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open() -> Result<Database> {
        let conn = Connection::open("/tmp/ytdl3.sqlite3")?;
        // let conn = Connection::open_in_memory()?; // TODO: File

        conn.execute(
            "CREATE TABLE IF NOT EXISTS channel (
                      id            INTEGER PRIMARY KEY AUTOINCREMENT,
                      name          TEXT NOT NULL,
                      service       TEXT NOT NULL
                      )",
            params![],
        )
        .context("Creating channel table")?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS video (
                      internalid    INTEGER PRIMARY KEY AUTOINCREMENT,
                      channel       INTEGER NOT NULL,
                      id            TEXT NOT NULL,
                      title         TEXT NOT NULL,
                      description   TEXT NOT NULL,
                      thumbnail     TEXT NOT NULL,
                      FOREIGN KEY(channel) REFERENCES channel(id)
                      )",
            params![],
        )
        .context("Creating video table")?;

        Ok(Database { conn })
    }

    pub fn insert(&self, video: &VideoInfo, chan: &YoutubeID) -> Result<()> {
        let chan_sqlid: i64 = self
            .conn
            .query_row(
                "SELECT id FROM channel WHERE name=?1 AND service = ?2",
                params![chan.id, chan.service_str()],
                |row| row.get(0),
            )
            .or_else(|_err| {
                // FIXME: This will create new entry if above query fails for any reason, not just doesn't exist
                self.conn.execute(
                    "INSERT INTO channel (name, service) VALUES (?1, ?2)",
                    params![chan.id, chan.service_str()],
                )?;
                let x: Result<i64> = Ok(self.conn.last_insert_rowid());
                x
            })
            .context("Failed to get (or create) channel ID")?;

        self.conn.execute(
            "INSERT INTO video (channel, id, title, description, thumbnail)
                VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                chan_sqlid,
                video.id,
                video.title,
                video.description,
                video.thumbnail_url,
            ],
        )?;

        Ok(())
    }
}

// let mut stmt = conn.prepare("SELECT id, name, time_created, data FROM person").unwrap();
// let person_iter = stmt.query_map(params![], |row| {
//     Ok(Person {
//         id: row.get(0).unwrap(),
//         name: row.get(1).unwrap(),
//         time_created: row.get(2).unwrap(),
//         data: row.get(3).unwrap(),
//     })
// }).unwrap();
