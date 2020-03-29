use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::{
    common::{Service, VideoStatus},
    config::Config,
    db::{DBVideoInfo, Database},
    youtube::VideoInfo,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct BackupChannel {
    chanid: String,
    service: String,
    videos: Vec<BackupVideoInfo>,
    icon: String,
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
        }
    }
}

impl From<&DBVideoInfo> for BackupVideoInfo {
    fn from(src: &DBVideoInfo) -> Self {
        Self {
            status: src.status.as_str().into(),
            title: src.info.title.clone(),
            url: src.info.url.clone(),
            videoid: src.info.id.clone(),
            publishdate: src.info.published_at.to_rfc3339(),
            description: src.info.description.clone(),
            thumbnail_url: src.info.thumbnail_url.clone(),
        }
    }
}

pub fn import() -> Result<()> {
    let cfg = Config::load();
    let db = Database::open(&cfg)?;

    let stdin = std::io::stdin();
    let lock = stdin.lock();
    let hm: Vec<BackupChannel> = serde_json::from_reader(lock)?;
    for chan in hm {
        let service = Service::from_str(&chan.service)?;
        let cid = service.get_channel_id(&chan.chanid);
        eprintln!("Processing cid = {:#?}", cid);
        let db_chan = crate::db::Channel::get(&db, &cid)
            .or_else(|_| crate::db::Channel::create(&db, &cid, &chan.chanid, &chan.icon))?;

        println!("Parsing videos");
        for backup_vid in chan.videos {
            let status = VideoStatus::from_str(&backup_vid.status)?;
            let v: VideoInfo = backup_vid.into();
            match db_chan.add_video(&db, &v) {
                Ok(dbv) => dbv.set_status(&db, status)?,
                Err(e) => eprintln!("{:?}", e),
            }
        }
    }
    Ok(())
}

pub fn export(output: Option<&str>) -> Result<()> {
    let cfg = Config::load();
    let db = Database::open(&cfg)?;

    let all = crate::db::all_videos(&db, std::i64::MAX, 0)?;
    let serialisable: Vec<BackupVideoInfo> = all.iter().map(|v| v.into()).collect();

    let stdout = std::io::stdout();
    if let Some(output) = output {
        let f = std::fs::File::create(output)?;
        serde_json::to_writer_pretty(f, &serialisable)?;
    } else {
        serde_json::to_writer_pretty(stdout.lock(), &serialisable)?;
    };

    Ok(())
}
