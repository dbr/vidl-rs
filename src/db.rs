use anyhow::{Context, Result};
use rusqlite::types::FromSql;
use rusqlite::{params, Connection};

use crate::youtube::VideoInfo;

/// Wraps connection to a database
pub struct Database {
    pub conn: Connection,
}

impl Database {
    /// Opens connection to database, creating tables as necessary
    pub fn open() -> Result<Database> {
        let conn = Connection::open("/tmp/ytdl3.sqlite3")?; // FIXME: Better location

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
                      published_at  DATETIME NOT NULL,
                      FOREIGN KEY(channel) REFERENCES channel(id)
                      )",
            params![],
        )
        .context("Creating video table")?;

        Ok(Database { conn })
    }
}

/// Supported services
#[derive(Debug, Clone, Copy)]
pub enum Service {
    Youtube,
    Vimeo,
}

impl Service {
    pub fn as_str(&self) -> &str {
        match self {
            Service::Youtube => "youtube",
            Service::Vimeo => "vimeo",
        }
    }
    pub fn from_str(name: &str) -> Result<Self> {
        match name {
            "youtube" => Ok(Service::Youtube),
            "vimeo" => Ok(Service::Vimeo),
            _ => Err(anyhow::anyhow!("Unknown service string {:?}", name)),
        }
    }
}

/// Converison from SQL text to `Service` instance
impl FromSql for Service {
    fn column_result(value: rusqlite::types::ValueRef) -> rusqlite::types::FromSqlResult<Self> {
        let sv: &str = value.as_str()?;
        let service = Service::from_str(sv).unwrap();
        Ok(service)
    }
}

/// Channel which contains a bunch of videos
#[derive(Debug)]
pub struct Channel {
    /// SQL ID number
    pub id: i64,
    /// ID of the video with the given service
    pub chanid: String,
    /// Which service the channel is on
    pub service: Service,
}

impl Channel {
    /// Get Channel object for given channel, creating it in database if not already present
    pub fn get_or_create(db: &Database, chanid: &str, service: Service) -> Result<Channel> {
        let chan_sqlid: i64 = db
            .conn
            .query_row(
                "SELECT id FROM channel WHERE name=?1 AND service = ?2",
                params![chanid, service.as_str()],
                |row| row.get(0),
            )
            .or_else(|_err| {
                // FIXME: This will create new entry if above query fails for any reason, not just doesn't exist
                db.conn.execute(
                    "INSERT INTO channel (name, service) VALUES (?1, ?2)",
                    params![chanid, service.as_str()],
                )?;
                let x: Result<i64> = Ok(db.conn.last_insert_rowid());
                x
            })
            .context("Failed to get (or create) channel ID")?;

        Ok(Channel {
            id: chan_sqlid,
            chanid: chanid.into(),
            service: service,
        })
    }

    /// Add supplied video to database
    pub fn add_video(&self, db: &Database, video: &VideoInfo) -> Result<()> {
        db.conn.execute(
            "INSERT INTO video (channel, id, title, description, thumbnail, published_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                self.id,
                video.id,
                video.title,
                video.description,
                video.thumbnail_url,
                video.published_at.to_rfc3339(),
            ],
        )?;

        Ok(())
    }

    /// Return the most recently published video
    pub fn latest_video(&self, db: &Database) -> Result<Option<VideoInfo>> {
        let v: Result<VideoInfo, rusqlite::Error> = db.conn.query_row(
            "SELECT id, title, description, thumbnail, published_at FROM video
                WHERE channel=?1
                ORDER BY published_at DESC
                LIMIT 1",
            params![self.id],
            |row| {
                Ok(VideoInfo {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    description: row.get(2)?,
                    thumbnail_url: row.get(3)?,
                    published_at: row.get(4)?,
                })
            },
        );

        match v {
            // Success
            Ok(video) => Ok(Some(video)),
            // No results
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            // Propagate other errors
            Err(e) => Err(e.into()),
        }
    }

    pub fn all_videos(&self, db: &Database) -> Result<Vec<VideoInfo>> {
        let mut stmt = db.conn.prepare(
            "SELECT id, title, description, thumbnail, published_at FROM video
            WHERE channel=?1
            ORDER BY published_at DESC",
        )?;
        let chaniter = stmt.query_map(params![self.id], |row| {
            Ok(VideoInfo {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                thumbnail_url: row.get(3)?,
                published_at: row.get(4)?,
            })
        })?;
        let mut ret = vec![];
        for r in chaniter {
            ret.push(r?);
        }
        Ok(ret)
    }
}

/// All channels present in database
pub fn list_channels(db: &Database) -> Result<Vec<Channel>> {
    let mut stmt = db.conn.prepare("SELECT id, name, service FROM channel")?;
    let chaniter = stmt.query_map(params![], |row| {
        Ok(Channel {
            id: row.get(0)?,
            chanid: row.get(1)?,
            service: row.get(2)?,
        })
    })?;
    let mut ret = vec![];
    for r in chaniter {
        ret.push(r?);
    }
    Ok(ret)
}
