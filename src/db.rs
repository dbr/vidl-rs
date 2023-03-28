use std::collections::HashSet;

use anyhow::{Context, Result};
use log::{debug, error, trace};
use rusqlite::types::FromSql;
use rusqlite::{params, Connection};
use thiserror::Error;

use crate::common::{ChannelID, Service, VideoStatus};
use crate::config::Config;
use crate::source::base::ChannelData;
use crate::source::base::{ChannelMetadata, VideoInfo};
use crate::source::invidious::YoutubeQuery;

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Invalid service string in database {0}")]
    InvalidServiceInDB(String),

    #[error("Invalid status string in database {0}")]
    InvalidStatusInDB(String),
}

#[derive(Debug)]
/// `VideoInfo` but with additional info which couldn't be known without the database (e.g SQL ID, VIDL's video status)
pub struct DBVideoInfo {
    /// SQL ID of video
    pub id: i64,

    /// The video info
    pub info: VideoInfo,

    /// If the video has been grabbed etc
    pub status: VideoStatus,

    /// SQL ID of parent channel
    pub chanid: i64,

    /// When it was added to the VIDL database (not to be confused with the `published_at` date on `VideoInfo`)
    pub date_added: chrono::DateTime<chrono::Utc>,
}

impl DBVideoInfo {
    /// Retrieve video's info by SQL ID
    pub fn get_by_sqlid(db: &Database, id: i64) -> Result<DBVideoInfo> {
        let chan = db
            .conn
            .query_row(
                "SELECT id, status, video_id, url, title, description, thumbnail, published_at, channel, duration, date_added, title_alt FROM video
                WHERE id=?1",
                params![id],
                |row| {
                    Ok(DBVideoInfo {
                        id: row.get("id")?,
                        status: row.get("status")?,
                        date_added: row.get("date_added")?,
                        info: VideoInfo {
                            id: row.get("video_id")?,
                            url: row.get("url")?,
                            title: row.get("title")?,
                            title_alt: row.get("title_alt")?,
                            description: row.get("description")?,
                            thumbnail_url: row.get("thumbnail")?,
                            published_at: row.get("date_added")?,
                            duration: row.get("duration")?,
                        },
                        chanid: row.get(8)?,
                    })
                },
            )
            .context("Failed to find channel")?;

        Ok(chan)
    }

    /// Get parent channel for video
    pub fn channel(&self, db: &Database) -> Result<Channel> {
        let chan = Channel::get_by_sqlid(&db, self.chanid)?;
        Ok(chan)
    }

    /// Set status of video
    pub fn set_status(&self, db: &Database, status: VideoStatus) -> Result<()> {
        // Update DB
        db.conn
            .execute(
                "UPDATE video SET status=?1 WHERE id=?2",
                params![status.as_str(), self.id],
            )
            .context("Failed to update video status")?;

        // FIXME: Should this update self.status?

        Ok(())
    }
}

/// Wraps connection to a database
pub struct Database {
    pub conn: Connection,
}

impl Database {
    fn connect(cfg: &Config, create: bool) -> Result<Connection> {
        let path = cfg.db_filepath();
        if let Some(p) = path.parent() {
            if !path.exists() {
                debug!("Creating {:?}", p);
                std::fs::create_dir_all(p)?
            }
        };
        debug!("Loading DB from {:?}", path);

        use rusqlite::OpenFlags;
        let flags = if create {
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
        } else {
            OpenFlags::SQLITE_OPEN_READ_WRITE
        };
        let conn = Connection::open_with_flags(path, flags)?;

        Ok(conn)
    }

    /// Create a new database
    pub fn create(cfg: &Config) -> Result<Database> {
        // Create new database
        let conn = Database::connect(&cfg, true)?;

        // Create tables
        let mig = crate::db_migration::get_migrator(&conn);
        mig.setup()?;
        mig.upgrade()?;

        // Return connection
        Ok(Database { conn })
    }

    /// Opens connection to database. Will throw error if schema is updated (can be updated with `Database::migrate`)
    pub fn open(cfg: &Config) -> Result<Database> {
        let conn = Database::connect(&cfg, false)?;

        let mig = crate::db_migration::get_migrator(&conn);
        mig.setup()?;

        if !mig.is_db_current()? {
            return Err(anyhow::anyhow!(
                "Database schema is incorrect version, currently {:?} should be {:?}",
                mig.get_db_version()?,
                mig.get_latest_version(),
            ));
        }

        Ok(Database { conn })
    }

    /// Upgrade database to latest schema version
    pub fn migrate(cfg: &Config) -> Result<()> {
        let conn = Database::connect(&cfg, false)?;

        let mig = crate::db_migration::get_migrator(&conn);
        mig.setup()?;

        mig.upgrade()?;

        Ok(())
    }

    /// Opens a non-persistant database in memory. Likely only useful for test cases.
    #[cfg(test)]
    pub fn create_in_memory(with_tables: bool) -> Result<Database> {
        // Create database in memory
        let conn = Connection::open_in_memory()?;

        if with_tables {
            // Setup migrator table
            let mig = crate::db_migration::get_migrator(&conn);
            mig.setup()?;

            // Setup latest schema
            mig.upgrade()?;
        }

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
                        id: row.get("id")?,
                        chanid: row.get("chanid")?,
                        service: row.get("service")?,
                        title: row.get("title")?,
                        thumbnail: row.get("thumbnail")?,
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
                        id: row.get("id")?,
                        chanid: row.get("chanid")?,
                        service: row.get("service")?,
                        title: row.get("title")?,
                        thumbnail: row.get("thumbnail")?,
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

    pub fn last_update(&self, db: &Database) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
        let result: Option<chrono::DateTime<chrono::Utc>> = db.conn.query_row(
            "SELECT last_update FROM channel WHERE id=?1",
            params![self.id],
            |row| Ok(row.get("last_update")?),
        )?;
        Ok(result)
    }

    /// Set the `last_update` time to now
    pub fn set_last_update(&self, db: &Database) -> Result<()> {
        let now = chrono::Utc::now();
        db.conn
            .execute(
                "UPDATE channel SET last_update=?1 WHERE id=?2",
                params![now, self.id],
            )
            .context("Failed to update last_update time")?;
        Ok(())
    }

    /// Determines if an update for this channel is due based on `last_update` time
    pub fn update_required(&self, db: &Database) -> Result<bool> {
        let last_update = self.last_update(&db)?;
        match last_update {
            Some(last_update) => {
                let now = chrono::Utc::now();
                let delta = now - last_update;
                let due_for_update = delta > chrono::Duration::minutes(60);
                let shedule_due = if due_for_update {
                    // FIXME: Something like chan.id % 60 == current_minute
                    true
                } else {
                    false
                };
                Ok(shedule_due)
            }
            None => Ok(true),
        }
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
        db.conn
            .execute(
                "INSERT INTO video (channel, video_id, url, title, description, thumbnail, published_at, status, duration, date_added)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    self.id,
                    video.id,
                    video.url,
                    video.title,
                    video.description,
                    video.thumbnail_url,
                    video.published_at.to_rfc3339(),
                    VideoStatus::New.as_str(), // Default status
                    video.duration,
                    chrono::Utc::now(),
                ],
            )
            .context("Add video query")?;
        let last_id = db.conn.last_insert_rowid();

        Ok(DBVideoInfo::get_by_sqlid(&db, last_id)?)
    }

    /// Get the URL's of the most recently published videos - returning up to and including `num` results.
    pub fn last_n_video_urls(&self, db: &Database, num: i64) -> Result<HashSet<String>> {
        let mut q = db.conn.prepare(
            "SELECT url FROM video
                WHERE channel=?1
                ORDER BY published_at DESC
                LIMIT ?2",
        )?;
        let mapped = q.query_map(params![self.id, num], |row| row.get("url"))?;

        let mut set = HashSet::new();
        for m in mapped {
            let url: String = m?;
            set.insert(url);
        }

        Ok(set)
    }

    pub fn all_videos(
        &self,
        db: &Database,
        limit: i64,
        page: i64,
        filter: Option<FilterParams>,
    ) -> Result<Vec<DBVideoInfo>> {
        let filter = match filter {
            Some(f) => Some(FilterParams {
                name_contains: f.name_contains,
                status: f.status,
                chanid: Some(self.id),
            }),
            None => Some(FilterParams {
                name_contains: None,
                status: None,
                chanid: Some(self.id),
            }),
        };

        all_videos(&db, limit, page, filter)
    }

    pub fn update(&self, db: &Database, full_update: bool) -> Result<()> {
        // Set updated time now (even in case of failure)
        self.set_last_update(&db)?;

        let mut chanid = crate::common::YoutubeID {
            id: self.chanid.clone(),
        };

        match self.service{
            Service::Youtube => {
                if ! self.chanid.starts_with("UC"){
                    let yt = crate::source::invidious::workaround::Yt::new();
                    if let Ok(fixed_id) = yt.find_channel_id(&self.chanid) {
                        log::info!("Updating chanid {} username/channel-name to Youtube ID {}", self.chanid, fixed_id);
                        db.conn.execute(
                            "UPDATE channel SET chanid = ?1 WHERE id = ?2",
                            params![fixed_id, self.id])?;
                        chanid.id = fixed_id;
                    } else {
                        log::error!("Failed to update channel id {}", self.chanid);
                    }
                }        
            },
            Service::Vimeo => {},
        }

        let api: Box<dyn ChannelData> = match self.service {
            Service::Youtube => Box::new(YoutubeQuery::new(&chanid)),
            Service::Vimeo => {
                // FIXME
                error!("Ignoring Vimeo channel {:?}", &self);
                return Ok(());
            }
        };

        let meta = api.get_metadata();

        match meta {
            Ok(meta) => self.update_metadata(&db, &meta)?,
            Err(e) => {
                error!(
                    "Error fetching metadata for {:?} - {} - skipping channel",
                    chanid, e
                );
                // Skip to next channel
                return Ok(());
            }
        }

        let seen_videos = self
            .last_n_video_urls(&db, 200)
            .context("Failed to find latest video URLs")?;

        trace!("Last seen video URL's: {:?}", &seen_videos);

        let mut new_videos: Vec<crate::source::base::VideoInfo> = vec![];

        for v in api.videos() {
            let v = v?;

            if seen_videos.contains(&v.url) && !full_update {
                debug!("Already seen video by URL {:?}", v.url);
                break;
            }

            trace!("New video {:?}", &v);
            new_videos.push(v);
        }

        for v in new_videos {
            debug!("Adding {0}", v.title);
            trace!("{:?}", &v);
            // TODO: Stop on "already seen video" error
            match self.add_video(&db, &v) {
                Ok(_) => (),
                Err(e) => error!("Error adding video {:?} - {:?}", &v, e),
            };
        }
        Ok(())
    }

    /// Deletes channel and all videos it contains
    pub fn delete(self, db: &Database) -> Result<()> {
        db.conn
            .execute("DELETE FROM video WHERE channel=?1", params![self.id])
            .context("Failed to delete videos in channel")?;

        db.conn
            .execute("DELETE FROM channel WHERE id=?1", params![self.id])
            .context("Failed to delete channel")?;

        Ok(())
    }
}

/// All channels present in database
pub fn list_channels(db: &Database) -> Result<Vec<Channel>> {
    let mut stmt = db
        .conn
        .prepare("SELECT id, chanid, service, title, thumbnail FROM channel ORDER BY title")?;
    let chaniter = stmt.query_map(params![], |row| {
        Ok(Channel {
            id: row.get("id")?,
            chanid: row.get("chanid")?,
            service: row.get("service")?,
            title: row.get("title")?,
            thumbnail: row.get("thumbnail")?,
        })
    })?;
    let mut ret = vec![];
    for r in chaniter {
        ret.push(r?);
    }
    Ok(ret)
}

pub struct FilterParams {
    pub name_contains: Option<String>,
    pub status: Option<HashSet<VideoStatus>>,
    pub chanid: Option<i64>,
}

pub fn all_videos(
    db: &Database,
    limit: i64,
    page: i64,
    filter: Option<FilterParams>,
) -> Result<Vec<DBVideoInfo>> {
    let mapper = |row: &rusqlite::Row| {
        Ok(DBVideoInfo {
            id: row.get("id")?,
            status: row.get("status")?,
            date_added: row.get("date_added")?,
            info: VideoInfo {
                id: row.get("video_id")?,
                url: row.get("url")?,
                title: row.get("title")?,
                title_alt: row.get("title_alt")?,
                description: row.get("description")?,
                thumbnail_url: row.get("thumbnail")?,
                published_at: row.get("published_at")?,
                duration: row.get("duration")?,
            },
            chanid: row.get("channel")?,
        })
    };

    let mut ret: Vec<DBVideoInfo> = vec![];

    // Create query snippet like:
    // (status = "NE" OR status = "GE")
    // Or `1` as placeholder if no statuses are set.
    let status_pred: String = if let Some(ref filter) = filter {
        if let Some(status) = &filter.status {
            let s = status
                .iter()
                .map(|s| format!(r#"status = "{}""#, s.as_str()))
                .collect::<Vec<String>>()
                .join(" OR ");

            if status.len() > 1 {
                format!("({})", s)
            } else {
                s
            }
        } else {
            "1".into() // 1 i.e true
        }
    } else {
        "1".into() // 1 i.e true
    };

    let chanid_pred: String = if let Some(ref filter) = filter {
        if let Some(cid) = filter.chanid {
            format!("channel = {}", cid)
        } else {
            "1".into()
        }
    } else {
        "1".into()
    };

    let sql = format!(
        r#"SELECT id, status, video_id, url, title, title_alt, description, thumbnail, published_at, channel, duration, date_added
        FROM video
        WHERE title LIKE ("%" || ?3 || "%")
            AND {}
            AND {}
        ORDER BY published_at DESC
        LIMIT ?1
        OFFSET ?2
        "#,
        status_pred, chanid_pred,
    );

    trace!("all_videos query SQL {}", &sql);

    let mut q = db.conn.prepare(&sql)?;
    let mapped = q.query_map(
        params![
            limit,
            page * limit,
            filter.and_then(|x| x.name_contains).unwrap_or("".into()),
        ],
        mapper,
    )?;
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
        // Create database in memory
        let mdb = Database::create_in_memory(true)?;

        // Check no channels exist
        {
            let chans = list_channels(&mdb)?;

            // Setup latest schema
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
            let vids = c.all_videos(&mdb, 50, 0, None)?;
            assert_eq!(vids.len(), 0);
        }

        // ..and no latest video
        {
            let latest = c.last_n_video_urls(&mdb, 50)?;
            assert_eq!(latest.len(), 0);
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
                duration: 12341,
            };
            c.add_video(&mdb, &new_video)?;
        }

        // Check video now exists
        {
            let vids = c.all_videos(&mdb, 50, 0, None)?;
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
            );
            assert_eq!(first.duration, 12341);
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
                duration: 0,
            };
            c.add_video(&mdb, &new_video)?;
        }

        // Check latest video method returns the older video, not oldest-inserted
        {
            let latest = c.last_n_video_urls(&mdb, 50)?;
            assert_eq!(latest.len(), 2);
        }
        Ok(())
    }

    #[test]
    fn test_filter() -> Result<()> {
        use crate::common::VideoStatus;

        let mdb = Database::create_in_memory(true)?;

        let cid = ChannelID::Youtube(crate::common::YoutubeID {
            id: "testchannel".into(),
        });

        let c = Channel::create(
            &mdb,
            &cid,
            "test channel",
            "http://example.com/thumbnail.jpg",
        )?;

        // Create new video
        {
            let when = chrono::DateTime::parse_from_rfc3339("2001-12-30T16:39:57Z")?
                .with_timezone(&chrono::Utc);

            let new_video = VideoInfo {
                id: "an id".into(),
                url: "http://example.com/watch?v=abc123".into(),
                title: "Good video!".into(),
                description: "A ficticious video.\nIt is quite good".into(),
                thumbnail_url: "http://example.com/vidthumb.jpg".into(),
                published_at: when,
                duration: 12341,
            };
            c.add_video(&mdb, &new_video)?;
        }

        // Video 2
        {
            let when = chrono::DateTime::parse_from_rfc3339("2001-12-30T16:39:57Z")?
                .with_timezone(&chrono::Utc);

            let new_video = VideoInfo {
                id: "an id".into(),
                url: "http://example.com/watch?v=def321".into(),
                title: "Another good video!".into(),
                description: "A ficticious video.\nIt is quite good".into(),
                thumbnail_url: "http://example.com/vidthumb.jpg".into(),
                published_at: when,
                duration: 12341,
            };
            c.add_video(&mdb, &new_video)?;
        }

        // Video 3
        {
            let when = chrono::DateTime::parse_from_rfc3339("2001-12-30T16:39:57Z")?
                .with_timezone(&chrono::Utc);

            let new_video = VideoInfo {
                id: "an id".into(),
                url: "http://example.com/watch?v=xyz789".into(),
                title: "A grab error".into(),
                description: "A ficticious video.\nIt is quite good".into(),
                thumbnail_url: "http://example.com/vidthumb.jpg".into(),
                published_at: when,
                duration: 12341,
            };
            let v = c.add_video(&mdb, &new_video)?;
            v.set_status(&mdb, crate::common::VideoStatus::GrabError)?;
        }

        // Searching by status only
        {
            let mut st = HashSet::new();
            st.insert(VideoStatus::GrabError);
            assert_eq!(
                all_videos(
                    &mdb,
                    99,
                    0,
                    Some(FilterParams {
                        name_contains: None,
                        status: Some(st),
                        chanid: None,
                    })
                )?
                .len(),
                1
            );
        }

        // Searching by another status
        {
            let mut st = HashSet::new();
            st.insert(VideoStatus::New);
            assert_eq!(
                all_videos(
                    &mdb,
                    99,
                    0,
                    Some(FilterParams {
                        name_contains: None,
                        status: Some(st),
                        chanid: None,
                    })
                )?
                .len(),
                2
            );
        }

        // Searching by status which has no videos
        {
            let mut st = HashSet::new();
            st.insert(VideoStatus::Downloading);
            assert_eq!(
                all_videos(
                    &mdb,
                    99,
                    0,
                    Some(FilterParams {
                        name_contains: None,
                        status: Some(st),
                        chanid: None,
                    })
                )?
                .len(),
                0
            );
        }

        // Searching by title only
        {
            let mut st = HashSet::new();
            st.insert(VideoStatus::New);
            assert_eq!(
                all_videos(
                    &mdb,
                    99,
                    0,
                    Some(FilterParams {
                        name_contains: Some("Another".into()),
                        status: Some(st),
                        chanid: None,
                    })
                )?
                .len(),
                1
            );
        }

        // Another search by title
        {
            let mut st = HashSet::new();
            st.insert(VideoStatus::New);
            assert_eq!(
                all_videos(
                    &mdb,
                    99,
                    0,
                    Some(FilterParams {
                        name_contains: Some("A".into()),
                        status: None,
                        chanid: None,
                    })
                )?
                .len(),
                2
            );
        }

        // Search by title finds nothing
        {
            let mut st = HashSet::new();
            st.insert(VideoStatus::New);
            assert_eq!(
                all_videos(
                    &mdb,
                    99,
                    0,
                    Some(FilterParams {
                        name_contains: Some("Blahblah".into()),
                        status: None,
                        chanid: None,
                    })
                )?
                .len(),
                0
            );
        }

        // No filtering
        {
            let mut st = HashSet::new();
            st.insert(VideoStatus::New);
            assert_eq!(all_videos(&mdb, 99, 0, None)?.len(), 3);
        }

        // Filtering with no specified parameters
        {
            let mut st = HashSet::new();
            st.insert(VideoStatus::New);
            assert_eq!(
                all_videos(
                    &mdb,
                    99,
                    0,
                    Some(FilterParams {
                        name_contains: None,
                        status: None,
                        chanid: None,
                    })
                )?
                .len(),
                3
            );
        }

        // Good
        Ok(())
    }

    #[test]
    fn test_deleting() -> Result<()> {
        let mdb = Database::create_in_memory(true)?;

        let c = Channel::create(
            &mdb,
            &ChannelID::Youtube(crate::common::YoutubeID {
                id: "testchannel".into(),
            }),
            "test channel",
            "http://example.com/thumbnail.jpg",
        )?;

        let c2 = Channel::create(
            &mdb,
            &ChannelID::Youtube(crate::common::YoutubeID {
                id: "secondchannel".into(),
            }),
            "second channel",
            "http://example.com/second.jpg",
        )?;

        // Create new video
        {
            let when = chrono::DateTime::parse_from_rfc3339("2001-12-30T16:39:57Z")?
                .with_timezone(&chrono::Utc);

            let new_video = VideoInfo {
                id: "1st".into(),
                url: "http://example.com/watch?v=abc123".into(),
                title: "Good video!".into(),
                description: "A ficticious video.\nIt is quite good".into(),
                thumbnail_url: "http://example.com/vidthumb.jpg".into(),
                published_at: when,
                duration: 12341,
            };
            dbg!("first");
            c.add_video(&mdb, &new_video)?;
        }

        // Video 2
        {
            let when = chrono::DateTime::parse_from_rfc3339("2001-12-30T16:39:57Z")?
                .with_timezone(&chrono::Utc);

            let new_video = VideoInfo {
                id: "2nd".into(),
                url: "http://example.com/watch?v=def321".into(),
                title: "Another good video!".into(),
                description: "A ficticious video.\nIt is quite good".into(),
                thumbnail_url: "http://example.com/vidthumb.jpg".into(),
                published_at: when,
                duration: 12341,
            };
            dbg!("second");
            c.add_video(&mdb, &new_video)?;
        }

        // Video 3
        {
            let when = chrono::DateTime::parse_from_rfc3339("2001-12-30T16:39:57Z")?
                .with_timezone(&chrono::Utc);

            let new_video = VideoInfo {
                id: "3rd".into(),
                url: "http://example.com/watch?v=xyz7890".into(),
                title: "A grab error".into(),
                description: "A third video".into(),
                thumbnail_url: "http://example.com/vidthumb.jpg".into(),
                published_at: when,
                duration: 12341,
            };
            c2.add_video(&mdb, &new_video)?;
        }

        // Check there are three videos
        {
            let videos = all_videos(&mdb, 9999, 0, None)?;
            assert_eq!(videos.len(), 3);

            assert_eq!(c.all_videos(&mdb, 9999, 0, None)?.len(), 2);
            assert_eq!(c2.all_videos(&mdb, 9999, 0, None)?.len(), 1);
        }

        // Delete one channel
        c.delete(&mdb)?;

        // Check videos remain in other channels
        {
            let videos = all_videos(&mdb, 9999, 0, None)?;
            assert_eq!(videos.len(), 1);

            assert_eq!(c2.all_videos(&mdb, 9999, 0, None)?.len(), 1);
        }

        Ok(())
    }
}
