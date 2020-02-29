use anyhow::{Context, Result};
use log::debug;
use rusqlite::types::FromSql;
use rusqlite::{params, Connection};

use crate::common::{ChannelID, Service};
use crate::config::Config;
use crate::youtube::VideoInfo;

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
    pub fn open_in_memory() -> Result<Database> {
        let conn = Connection::open_in_memory()?;
        Database::create_tables(&conn)?;
        Ok(Database { conn })
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

    /// Human-readable title
    pub title: String,
    /// URL to icon for channel
    pub thumbnail: String,
}

impl Channel {
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

        db.conn.execute(
            "INSERT INTO channel (chanid, service, title, thumbnail) VALUES (?1, ?2, ?3, ?4)",
            params![
                cid.id_str(),
                cid.service().as_str(),
                channel_title,
                thumbnail_url,
            ],
        )?;

        // Return newly created channel
        Channel::get(&db, cid)
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
    let mut stmt = db
        .conn
        .prepare("SELECT id, chanid, service, title, thumbnail FROM channel")?;
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
            let vids = c.all_videos(&mdb)?;
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
                title: "A title!".into(),
                description: "A ficticious video.\nIt is quite good".into(),
                thumbnail_url: "http://example.com/vidthumb.jpg".into(),
                published_at: when,
            };
            c.add_video(&mdb, &new_video)?;
        }

        // Check video now exists
        {
            let vids = c.all_videos(&mdb)?;
            assert_eq!(vids.len(), 1);
            assert_eq!(vids[0].id, "an id");
            assert_eq!(vids[0].title, "A title!");
            assert_eq!(vids[0].description, "A ficticious video.\nIt is quite good");
            assert_eq!(vids[0].thumbnail_url, "http://example.com/vidthumb.jpg");
            assert_eq!(
                vids[0].published_at,
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
            assert_eq!(latest.unwrap().id, "an id");
        }
        Ok(())
    }
}
