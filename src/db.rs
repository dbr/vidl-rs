use anyhow::{Context, Result};
use log::debug;
use rusqlite::types::FromSql;
use rusqlite::{params, Connection};
use thiserror::Error;

use crate::common::{ChannelID, Service, VideoStatus};
use crate::config::Config;
use crate::youtube::{ChannelMetadata, VideoInfo};

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Misc database error")]
    DatabaseError,

    #[error("Invalid service string in database {0}")]
    InvalidServiceInDB(String),

    #[error("Invalid status string in database {0}")]
    InvalidStatusInDB(String),
}

#[derive(Debug)]
/// `VideoInfo` but with an SQL ID
pub struct DBVideoInfo {
    pub id: i64,
    pub info: VideoInfo,
    pub status: VideoStatus,
    pub chanid: i64,
}

impl DBVideoInfo {
    pub fn get_by_sqlid(db: &Database, id: i64) -> Result<DBVideoInfo> {
        let chan = db
            .conn
            .query_row(
                "SELECT id, status, video_id, url, title, description, thumbnail, published_at, channel FROM video
                WHERE id=?1",
                params![id],
                |row| {
                    Ok(DBVideoInfo {
                        id: row.get(0)?,
                        status: row.get(1)?,
                        info: VideoInfo {
                            id: row.get(2)?,
                            url: row.get(3)?,
                            title: row.get(4)?,
                            description: row.get(5)?,
                            thumbnail_url: row.get(6)?,
                            published_at: row.get(7)?,
                        },
                        chanid: row.get(8)?,
                    })
                },
            )
            .context("Failed to find channel")?;

        Ok(chan)
    }

    pub fn channel(&self, db: &Database) -> Result<Channel> {
        let chan = Channel::get_by_sqlid(&db, self.chanid)?;
        Ok(chan)
    }

    pub fn set_status(&self, db: &Database, status: VideoStatus) -> Result<()> {
        let chan = db
            .conn
            .execute(
                "UPDATE video SET status=?1 WHERE id=?2",
                params![status.as_str(), self.id],
            )
            .context("Failed to update video status")?;

        Ok(())
    }
}

/// Wraps connection to a database
pub struct Database {
    pub conn: Connection,
}

impl Database {
    fn create_tables(conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS channel (
                      id            INTEGER PRIMARY KEY AUTOINCREMENT,
                      chanid        TEXT NOT NULL,
                      service       TEXT NOT NULL,
                      title         TEXT NOT NULL,
                      thumbnail     TEXT NOT NULL
                      )",
            params![],
        )
        .context("Creating channel table")?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS video (
                      id            INTEGER PRIMARY KEY AUTOINCREMENT,
                      channel       INTEGER NOT NULL,
                      video_id      TEXT NOT NULL,
                      status        TEXT NOT NULL,
                      url           TEXT NOT NULL UNIQUE,
                      title         TEXT NOT NULL,
                      description   TEXT NOT NULL,
                      thumbnail     TEXT NOT NULL,
                      published_at  DATETIME NOT NULL,
                      FOREIGN KEY(channel) REFERENCES channel(id)
                      )",
            params![],
        )
        .context("Creating video table")?;

        Ok(())
    }
    /// Opens connection to database, creating tables as necessary
    pub fn open(cfg: &Config) -> Result<Database> {
        let path = cfg.db_filepath();
        if let Some(p) = path.parent() {
            debug!("Creating {:?}", p);
            std::fs::create_dir_all(p)?
        };
        debug!("Loading DB from {:?}", path);
        let conn = Connection::open(path)?;
        Database::create_tables(&conn)?;
        Ok(Database { conn })
    }

    /// Opens a non-persistant database in memory. Likely only useful for test cases.
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Database> {
        let conn = Connection::open_in_memory()?;
        Database::create_tables(&conn)?;
        Ok(Database { conn })
    }
}

/// Converison from SQL text to `Service` instance
impl FromSql for Service {
    fn column_result(
        value: rusqlite::types::ValueRef,
    ) -> Result<Self, rusqlite::types::FromSqlError> {
        let raw: &str = value.as_str()?;
        match Service::from_str(raw) {
            Ok(s) => Ok(s),
            Err(_e) => Err(rusqlite::types::FromSqlError::Other(Box::new(
                DatabaseError::InvalidServiceInDB(raw.into()),
            ))),
        }
    }
}

/// Converison from SQL text to `Service` instance
impl FromSql for VideoStatus {
    fn column_result(value: rusqlite::types::ValueRef) -> rusqlite::types::FromSqlResult<Self> {
        let raw: &str = value.as_str()?;
        match VideoStatus::from_str(raw) {
            Ok(s) => Ok(s),
            Err(_e) => Err(rusqlite::types::FromSqlError::Other(Box::new(
                DatabaseError::InvalidStatusInDB(raw.into()),
            ))),
        }
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

    /// Human-readable title
    pub title: String,
    /// URL to icon for channel
    pub thumbnail: String,
}

impl Channel {
    pub fn get_by_sqlid(db: &Database, id: i64) -> Result<Channel> {
        let chan = db
            .conn
            .query_row(
                "SELECT id, chanid, service, title, thumbnail FROM channel WHERE id=?1",
                params![id],
                |row| {
                    Ok(Channel {
                        id: row.get(0)?,
                        chanid: row.get(1)?,
                        service: row.get(2)?,
                        title: row.get(3)?,
                        thumbnail: row.get(4)?,
                    })
                },
            )
            .context("Failed to find channel")?;

        Ok(chan)
    }

    /// Get Channel object for given channel, returning error it it does not exist
    pub fn get(db: &Database, cid: &ChannelID) -> Result<Channel> {
        let chan = db.conn
            .query_row(
                "SELECT id, chanid, service, title, thumbnail FROM channel WHERE chanid=?1 AND service = ?2",
                params![cid.id_str(), cid.service().as_str()],
                |row| {
                    Ok(Channel {
                        id: row.get(0)?,
                        chanid: row.get(1)?,
                        service: row.get(2)?,
                        title: row.get(3)?,
                        thumbnail: row.get(4)?,
                    })
                },
            )
            .context("Failed to find channel")?;

        Ok(chan)
    }

    /// Create channel in database
    pub fn create(
        db: &Database,
        cid: &ChannelID,
        channel_title: &str,
        thumbnail_url: &str,
    ) -> Result<Channel> {
        let check_existing = db.conn.query_row(
            "SELECT id FROM channel WHERE chanid=?1 AND service=?2",
            params![cid.id_str(), cid.service().as_str()],
            |_| Ok(()),
        );
        match check_existing {
            // Throw error if channel already exists
            Ok(_) => Err(anyhow::anyhow!("Channel already exists in database")),

            // No results is good
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(()),

            // Propogate any other errors
            Err(e) => Err(anyhow::anyhow!(e)),
        }?;

        db.conn
            .execute(
                "INSERT INTO channel (chanid, service, title, thumbnail) VALUES (?1, ?2, ?3, ?4)",
                params![
                    cid.id_str(),
                    cid.service().as_str(),
                    channel_title,
                    thumbnail_url,
                ],
            )
            .context("Insert channel query")?;

        // Return newly created channel
        Channel::get(&db, cid)
    }

    pub fn update_metadata(&self, db: &Database, meta: &ChannelMetadata) -> Result<()> {
        db.conn
            .execute(
                "UPDATE channel SET title=?1, thumbnail=?2 WHERE id=?3",
                params![meta.title, meta.thumbnail, self.id],
            )
            .context("Failed to update channel metadata")?;
        Ok(())
    }

    /// Add supplied video to database
    pub fn add_video(&self, db: &Database, video: &VideoInfo) -> Result<DBVideoInfo> {
        match db.conn
            .execute(
                "INSERT INTO video (channel, video_id, url, title, description, thumbnail, published_at, status)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    self.id,
                    video.id,
                    video.url,
                    video.title,
                    video.description,
                    video.thumbnail_url,
                    video.published_at.to_rfc3339(),
                    VideoStatus::New.as_str(), // Default status
                ],
            )
            .context("Add video query") {
                Ok(k) => (Ok(k)),
                Err(e) => {
                    println!("{:?}", e);
                    Err(e)
                },
            }?;
        let last_id = db.conn.last_insert_rowid();

        Ok(DBVideoInfo::get_by_sqlid(&db, last_id)?)
    }

    /// Return the most recently published video
    pub fn latest_video(&self, db: &Database) -> Result<Option<DBVideoInfo>> {
        let v: Result<DBVideoInfo, rusqlite::Error> = db.conn.query_row(
            "SELECT id, status, video_id, url, title, description, thumbnail, published_at, channel FROM video
                WHERE channel=?1
                ORDER BY published_at DESC
                LIMIT 1",
            params![self.id],
            |row| {
                Ok(DBVideoInfo {
                    id: row.get(0)?,
                    status: row.get(1)?,
                    info: VideoInfo {
                        id: row.get(2)?,
                        url: row.get(3)?,
                        title: row.get(4)?,
                        description: row.get(5)?,
                        thumbnail_url: row.get(6)?,
                        published_at: row.get(7)?,
                    },
                    chanid: row.get(8)?,
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

    pub fn all_videos(&self, db: &Database, limit: i64, page: i64) -> Result<Vec<DBVideoInfo>> {
        let mapper = |row: &rusqlite::Row| {
            Ok(DBVideoInfo {
                id: row.get(0)?,
                status: row.get(1)?,
                info: VideoInfo {
                    id: row.get(2)?,
                    url: row.get(3)?,
                    title: row.get(4)?,
                    description: row.get(5)?,
                    thumbnail_url: row.get(6)?,
                    published_at: row.get(7)?,
                },
                chanid: row.get(8)?,
            })
        };

        let mut ret: Vec<DBVideoInfo> = vec![];

        let mut q = db.conn.prepare(
            "SELECT id, status, video_id, url, title, description, thumbnail, published_at, channel
                FROM video
                WHERE channel=?1
                ORDER BY published_at DESC
                LIMIT ?2
                OFFSET ?3
                ",
        )?;
        let mapped = q.query_map(params![self.id, limit, page * limit], mapper)?;
        for r in mapped {
            ret.push(r?);
        }
        Ok(ret)
    }
}

/// All channels present in database
pub fn list_channels(db: &Database) -> Result<Vec<Channel>> {
    let mut stmt = db
        .conn
        .prepare("SELECT id, chanid, service, title, thumbnail FROM channel ORDER BY title")?;
    let chaniter = stmt.query_map(params![], |row| {
        Ok(Channel {
            id: row.get(0)?,
            chanid: row.get(1)?,
            service: row.get(2)?,
            title: row.get(3)?,
            thumbnail: row.get(4)?,
        })
    })?;
    let mut ret = vec![];
    for r in chaniter {
        ret.push(r?);
    }
    Ok(ret)
}

pub fn all_videos(db: &Database, limit: i64, page: i64) -> Result<Vec<DBVideoInfo>> {
    let mapper = |row: &rusqlite::Row| {
        Ok(DBVideoInfo {
            id: row.get(0)?,
            status: row.get(1)?,
            info: VideoInfo {
                id: row.get(2)?,
                url: row.get(3)?,
                title: row.get(4)?,
                description: row.get(5)?,
                thumbnail_url: row.get(6)?,
                published_at: row.get(7)?,
            },
            chanid: row.get(8)?,
        })
    };

    let mut ret: Vec<DBVideoInfo> = vec![];

    let mut q = db.conn.prepare(
        "SELECT id, status, video_id, url, title, description, thumbnail, published_at, channel
            FROM video
            ORDER BY published_at DESC
            LIMIT ?1
            OFFSET ?2
            ",
    )?;
    let mapped = q.query_map(params![limit, page * limit], mapper)?;
    for r in mapped {
        ret.push(r?);
    }
    Ok(ret)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_channels() -> Result<()> {
        let mdb = Database::open_in_memory()?;

        // Check no channels exist in newly created DB
        {
            let chans = list_channels(&mdb)?;
            assert_eq!(chans.len(), 0);
        }

        // Create channel
        {
            let cid = ChannelID::Youtube(crate::common::YoutubeID {
                id: "UCUBfKCp83QT19JCUekEdxOQ".into(),
            });

            Channel::create(
                &mdb,
                &cid,
                "test channel",
                "http://example.com/thumbnail.jpg",
            )?;
        }

        let chans = list_channels(&mdb)?;
        assert_eq!(chans.len(), 1);
        assert_eq!(chans[0].id, 1);
        assert_eq!(chans[0].chanid, "UCUBfKCp83QT19JCUekEdxOQ");
        assert_eq!(chans[0].title, "test channel");
        assert_eq!(chans[0].thumbnail, "http://example.com/thumbnail.jpg");

        let c = &chans[0];

        // Check no videos exist
        {
            let vids = c.all_videos(&mdb, 50, 0)?;
            assert_eq!(vids.len(), 0);
        }

        // ..and no latest video
        {
            let latest = c.latest_video(&mdb)?;
            assert!(latest.is_none());
        }

        // Create new video
        {
            let when = chrono::DateTime::parse_from_rfc3339("2001-12-30T16:39:57Z")?
                .with_timezone(&chrono::Utc);

            let new_video = VideoInfo {
                id: "an id".into(),
                url: "http://example.com/watch?v=abc123".into(),
                title: "A title!".into(),
                description: "A ficticious video.\nIt is quite good".into(),
                thumbnail_url: "http://example.com/vidthumb.jpg".into(),
                published_at: when,
            };
            c.add_video(&mdb, &new_video)?;
        }

        // Check video now exists
        {
            let vids = c.all_videos(&mdb, 50, 0)?;
            assert_eq!(vids.len(), 1);
            let first = &vids[0].info;
            assert_eq!(first.id, "an id");
            assert_eq!(first.url, "http://example.com/watch?v=abc123");
            assert_eq!(first.title, "A title!");
            assert_eq!(first.description, "A ficticious video.\nIt is quite good");
            assert_eq!(first.thumbnail_url, "http://example.com/vidthumb.jpg");
            assert_eq!(
                first.published_at,
                chrono::DateTime::parse_from_rfc3339("2001-12-30T16:39:57Z")?
                    .with_timezone(&chrono::Utc)
            )
        }

        // Create "older" video for latest_video test
        {
            let when = chrono::DateTime::parse_from_rfc3339("1999-04-01T12:30:01Z")?
                .with_timezone(&chrono::Utc);

            let new_video = VideoInfo {
                id: "old id".into(),
                url: "http://example.com/watch?v=old".into(),
                title: "Old video".into(),
                description: "Was created a while ago".into(),
                thumbnail_url: "http://example.com/oldvid.jpg".into(),
                published_at: when,
            };
            c.add_video(&mdb, &new_video)?;
        }

        // Check latest video method returns the older video, not oldest-inserted
        {
            let latest = c.latest_video(&mdb)?;
            assert!(latest.is_some());
            assert_eq!(latest.unwrap().info.id, "an id");
        }
        Ok(())
    }
}
