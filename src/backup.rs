use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::common::{Service, VideoStatus};
use crate::config::Config;
use crate::db::{Channel, DBVideoInfo, Database};
use crate::youtube::VideoInfo;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct BackupChannel {
    chanid: String,
    service: String,
    icon: String,
    id: i64,
}

impl From<&Channel> for BackupChannel {
    fn from(src: &Channel) -> Self {
        Self {
            chanid: src.chanid.clone(),
            service: src.service.as_str().into(),
            icon: src.thumbnail.clone(),
            id: src.id,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct BackupVideoInfo {
    status: String,
    title: String,
    url: String,
    videoid: String,
    publishdate: String,
    description: String,
    thumbnail_url: String,
    channel_id: i64,
    duration: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Backup {
    channels: Vec<BackupChannel>,
    videos: Vec<BackupVideoInfo>,
}

impl From<BackupVideoInfo> for VideoInfo {
    fn from(src: BackupVideoInfo) -> Self {
        let when: DateTime<Utc> = DateTime::parse_from_rfc3339(&src.publishdate)
            .expect("Invalid date")
            .with_timezone(&Utc);

        Self {
            id: src.videoid,
            url: src.url,
            title: src.title,
            description: src.description,
            thumbnail_url: src.thumbnail_url,
            published_at: when,
            duration: src.duration,
        }
    }
}

impl From<&DBVideoInfo> for BackupVideoInfo {
    fn from(src: &DBVideoInfo) -> Self {
        Self {
            channel_id: src.chanid,
            status: src.status.as_str().into(),
            title: src.info.title.clone(),
            url: src.info.url.clone(),
            videoid: src.info.id.clone(),
            publishdate: src.info.published_at.to_rfc3339(),
            description: src.info.description.clone(),
            thumbnail_url: src.info.thumbnail_url.clone(),
            duration: src.info.duration,
        }
    }
}

/// Load backup file
pub fn import() -> Result<()> {
    let cfg = Config::load();
    let db = Database::open(&cfg)?;

    let stdin = std::io::stdin();
    let lock = stdin.lock();
    let back: Backup = serde_json::from_reader(lock)?;

    let mut backup_id_to_channel_mapper: HashMap<i64, Channel> = HashMap::new();
    for back_chan in back.channels {
        // Get service
        let service = Service::from_str(&back_chan.service)?;
        // Get channel ID
        let cid = service.get_channel_id(&back_chan.chanid);

        // Get or create channel
        let db_chan = crate::db::Channel::get(&db, &cid).or_else(|_| {
            crate::db::Channel::create(&db, &cid, &back_chan.chanid, &back_chan.icon)
        })?;

        // Create a mapping from backup-channel-id to database
        backup_id_to_channel_mapper.insert(back_chan.id, db_chan);
    }

    for backup_vid in back.videos {
        // Get channel object
        let db_chan = &backup_id_to_channel_mapper[&backup_vid.channel_id];

        // Parse video status
        let status = VideoStatus::from_str(&backup_vid.status)?;

        // Convert video
        let v: VideoInfo = backup_vid.into();

        // Insert into database
        match db_chan.add_video(&db, &v) {
            Ok(dbv) => dbv.set_status(&db, status)?,
            Err(e) => eprintln!("{:?}", e),
        }
    }
    Ok(())
}

/// Export channels, videos, and their status etc to a JSON file
pub fn export(output: Option<&str>) -> Result<()> {
    let cfg = Config::load();
    let db = Database::open(&cfg)?;

    let chans = crate::db::list_channels(&db)?;
    let chans_ser: Vec<BackupChannel> = chans.iter().map(|v| v.into()).collect();

    let vids = crate::db::all_videos(&db, std::i64::MAX, 0, None)?;
    let vids_ser: Vec<BackupVideoInfo> = vids.iter().map(|v| v.into()).collect();

    let back = Backup {
        channels: chans_ser,
        videos: vids_ser,
    };

    let stdout = std::io::stdout();
    if let Some(output) = output {
        let f = std::fs::File::create(output)?;
        serde_json::to_writer_pretty(f, &back)?;
    } else {
        serde_json::to_writer_pretty(stdout.lock(), &back)?;
    };

    Ok(())
}
