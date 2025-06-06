use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use askama::Template;
use lazy_static::lazy_static;
use log::info;
use rouille::{router, Request, Response};
use serde_derive::Serialize;

use crate::common::VideoStatus;
use crate::config::Config;
use crate::db::{Channel, DBVideoInfo, FilterParams};
use crate::worker::WorkerPool;

#[derive(Clone)]
pub(crate) struct Image {
    pub(crate) data: Vec<u8>,
    pub(crate) content_type: String,
}

pub(crate) struct ImageCache {
    images: HashMap<String, Image>,
}

#[derive(Clone)]
enum ImageCacheResponse {
    Redirect(String),
    Image(Image),
}

impl ImageCache {
    fn new() -> Self {
        ImageCache {
            images: HashMap::new(),
        }
    }

    pub(crate) fn contains(&self, url: &str) -> bool {
        self.images.contains_key(url)
    }

    fn get(
        &mut self,
        url: String,
        worker: Arc<Mutex<crate::worker::WorkerPool>>,
    ) -> Result<ImageCacheResponse> {
        if self.images.contains_key(&url) {
            let cached = self.images.get(&url);
            Ok(ImageCacheResponse::Image((*cached.unwrap()).clone()))
        } else {
            let thready_url: String = url.clone();
            let pool = worker.lock().unwrap();
            pool.enqueue(crate::worker::WorkItem::ThumbnailCache(thready_url));

            Ok(ImageCacheResponse::Redirect(url.into()))
        }
    }

    pub(crate) fn add(&mut self, url: &str, img: Image) {
        self.images.insert(url.into(), img);
    }
}

lazy_static! {
    pub(crate) static ref IMG_CACHE: Mutex<ImageCache> = Mutex::new(ImageCache::new());
}

#[derive(Debug, Serialize)]
pub struct WebChannelStats {
    grabbed: usize,
    new: usize,
    other: usize,
}

#[derive(Debug, Serialize)]
pub struct WebChannel {
    id: i64,
    chanid: String,
    service: String,
    title: String,
    icon: String,
    stats_1w: WebChannelStats,
    stats_all: WebChannelStats,
}

impl WebChannel {
    fn new(src: Channel, db: &crate::db::Database) -> Result<WebChannel> {
        let stats_1w = src.stats_1w(db)?.into();
        let stats_all = src.stats_all(db)?.into();
        Ok(WebChannel {
            id: src.id,
            chanid: src.chanid,
            service: src.service.as_str().into(),
            title: src.title,
            icon: src.thumbnail,
            stats_1w,
            stats_all,
        })
    }
}

impl From<crate::db::ChannelStats> for WebChannelStats {
    fn from(src: crate::db::ChannelStats) -> WebChannelStats {
        WebChannelStats {
            grabbed: src.grabbed,
            new: src.new,
            other: src.other,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct WebChannelList {
    channels: Vec<WebChannel>,
}

impl WebChannelList {
    fn new(src: Vec<Channel>, db: &crate::db::Database) -> Result<WebChannelList> {
        let mut channels: Vec<WebChannel> = vec![];
        for p in src {
            let c = WebChannel::new(p, db)?;
            channels.push(c);
        }
        Ok(WebChannelList { channels })
    }
}

#[derive(Debug, Serialize)]
pub struct WebVideoInfo<'a> {
    id: i64,
    video_id: String,
    url: String,
    title: String,
    title_alt: Option<String>,
    description: String,
    description_alt: Option<String>,
    thumbnail_url: String,
    published_at: String,
    status_class: String,
    channel: &'a WebChannel,
    duration: i32,
}

impl<'a> WebVideoInfo<'a> {
    pub fn video_duration_str(&self) -> String {
        format!("{}m{}", self.duration / 60, self.duration % 60)
    }

    pub fn get_title(&self) -> &str {
        if let Some(t) = &self.title_alt {
            &t
        } else {
            &self.title
        }
    }

    pub fn get_description_alt(&self) -> &str {
        match &self.description_alt {
            Some(t) => t,
            None => "None",
        }
    }
}

fn status_css_class(status: VideoStatus) -> String {
    match status {
        VideoStatus::New => "ytdl-new",
        VideoStatus::Queued => "ytdl-queued",
        VideoStatus::Downloading => "ytdl-downloading",
        VideoStatus::Grabbed => "ytdl-grabbed",
        VideoStatus::GrabError => "ytdl-graberror",
        VideoStatus::Ignore => "ytdl-ignore",
    }
    .into()
}

impl<'a> From<(DBVideoInfo, &'a WebChannel)> for WebVideoInfo<'a> {
    fn from(src: (DBVideoInfo, &'a WebChannel)) -> WebVideoInfo<'a> {
        let (src, chan) = src;
        WebVideoInfo {
            id: src.id,
            video_id: src.info.id,
            url: src.info.url,
            title: src.info.title,
            title_alt: src.info.title_alt,
            description: src.info.description,
            description_alt: src.info.description_alt,
            thumbnail_url: src.info.thumbnail_url,
            published_at: src.info.published_at.to_rfc3339(),
            status_class: status_css_class(src.status),
            channel: chan,
            duration: src.info.duration,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct WebChannelVideos<'a> {
    videos: Vec<(String, Vec<WebVideoInfo<'a>>)>,
}

#[derive(Template)]
#[template(path = "channel_list.html")]
struct ChannelListTemplate<'a> {
    chans: &'a WebChannelList,
}

fn page_chan_list() -> Result<Response> {
    let cfg = crate::config::Config::load();
    let db = crate::db::Database::open(&cfg)?;
    let chans = crate::db::list_channels(&db)?;
    let ret = WebChannelList::new(chans, &db)?;

    let t = ChannelListTemplate { chans: &ret };

    let html = t.render()?;
    Ok(Response::html(html))
}

#[derive(Template)]
#[template(path = "video_list.html")]
struct VideoListTemplate<'a> {
    videos: &'a WebChannelVideos<'a>,
    page: i64,
}

fn page_list_videos(
    id: Option<i64>,
    page: i64,
    filter: Option<FilterParams>,
    as_json: bool,
) -> Result<Response> {
    let cfg = crate::config::Config::load();
    let db = crate::db::Database::open(&cfg)?;
    let (c, videos): (Option<Channel>, Vec<DBVideoInfo>) = if let Some(id) = id {
        let c = crate::db::Channel::get_by_sqlid(&db, id)?;
        let videos = c.all_videos(&db, 50, page, filter)?;
        (Some(c), videos)
    } else {
        let videos = crate::db::all_videos(&db, 50, page, filter)?;
        (None, videos)
    };

    // Construct a map of WebChannel's to be referenced by each video
    let mut chans: HashMap<i64, WebChannel> = HashMap::new();
    if let Some(c) = c {
        chans.insert(c.id, WebChannel::new(c, &db)?);
    } else {
        for v in &videos {
            let c = v.channel(&db)?;
            chans.insert(c.id, WebChannel::new(c, &db)?);
        }
    }

    // Group by date
    let mut by_date_step1: BTreeMap<String, Vec<WebVideoInfo>> = BTreeMap::new();
    for v in videos {
        let timestamp = v.info.published_at.date().format("%Y-%m-%d").to_string();
        let wc = &chans[&v.chanid];
        by_date_step1
            .entry(timestamp)
            .or_insert_with(Vec::new)
            .push((v, wc).into());
    }
    // Each WebChannelVideo is VideoInfo plus a reference to the channel it belongs to

    // Reverse ordering of days - workaround for https://github.com/djc/askama/issues/333
    let by_date: Vec<(String, Vec<WebVideoInfo>)> = by_date_step1.into_iter().rev().collect();

    let ret: WebChannelVideos = WebChannelVideos { videos: by_date };

    if as_json {
        // Serialize the data into JSON
        let json_data = serde_json::json!({
            "videos": &ret,
            "page": page,
        });

        // Return JSON response
        Ok(Response::json(&json_data))
    } else {
        let t = VideoListTemplate {
            videos: &ret,
            page: page,
        };
        let html = t.render()?;
        Ok(Response::html(html))
    }
}

fn page_set_title_alt(videoid: i64, title: String) -> Result<Response> {
    let cfg = crate::config::Config::load();
    let db = crate::db::Database::open(&cfg)?;
    let v = crate::db::DBVideoInfo::get_by_sqlid(&db, videoid)?;
    v.set_title_alt(&db, title)?;
    Ok(Response::text("ok"))
}

fn page_set_description_alt(videoid: i64, title: String) -> Result<Response> {
    let cfg = crate::config::Config::load();
    let db = crate::db::Database::open(&cfg)?;
    let v = crate::db::DBVideoInfo::get_by_sqlid(&db, videoid)?;
    v.set_description_alt(&db, title)?;
    Ok(Response::text("ok"))
}

fn page_download_video(videoid: i64, workers: Arc<Mutex<WorkerPool>>) -> Result<Response> {
    let cfg = crate::config::Config::load();
    let db = crate::db::Database::open(&cfg)?;
    let v = crate::db::DBVideoInfo::get_by_sqlid(&db, videoid)?;
    let chanid = v.chanid;

    // Mark video as queued
    v.set_status(&db, VideoStatus::Queued)?;

    // Then add it to the work queue
    {
        let w = workers.lock().unwrap();
        w.enqueue(crate::worker::WorkItem::Download(v));
    }

    // Redirect to channel for no-javascript clicking
    Ok(Response::redirect_303(format!("/channel/{}", chanid)))
}

fn page_ignore_video(videoid: i64) -> Result<Response> {
    let cfg = crate::config::Config::load();
    let db = crate::db::Database::open(&cfg)?;
    let v = crate::db::DBVideoInfo::get_by_sqlid(&db, videoid)?;
    let chanid = v.chanid;

    // Mark video as queued
    v.set_status(&db, VideoStatus::Ignore)?;

    // Redirect to channel for no-javascript clicking
    Ok(Response::redirect_303(format!("/channel/{}", chanid)))
}

enum ThumbnailType {
    Video,
    Channel,
}

fn page_thumbnail(
    id: i64,
    what: ThumbnailType,
    workers: Arc<Mutex<WorkerPool>>,
) -> Result<Response> {
    let cfg = crate::config::Config::load();
    let db = crate::db::Database::open(&cfg)?;

    let url = match what {
        ThumbnailType::Channel => {
            let chan = crate::db::Channel::get_by_sqlid(&db, id)?;
            chan.thumbnail
        }
        ThumbnailType::Video => {
            let vi = crate::db::DBVideoInfo::get_by_sqlid(&db, id)?;
            vi.info.thumbnail_url
        }
    };

    let full_url = {
        if url.starts_with("http://") || url.starts_with("https://") {
            url
        } else {
            let prefix_hackery: String = std::env::var("VIDL_INVIDIOUS_URL").unwrap();
            format!("{prefix_hackery}{url}")
        }
    };

    let image = {
        let mut ic = IMG_CACHE.lock().unwrap();
        ic.get(full_url, workers)?
    };
    match image {
        ImageCacheResponse::Redirect(url) => Ok(Response::redirect_303(url)),
        ImageCacheResponse::Image(image) => Ok(Response::from_data(image.content_type, image.data)),
    }
}

fn page_refresh(workers: Arc<Mutex<WorkerPool>>) -> Result<Response> {
    let cfg = crate::config::Config::load();
    let db = crate::db::Database::open(&cfg)?;

    // Then add it to the work queue
    {
        let channels = crate::db::list_channels(&db)?;
        let w = workers.lock().unwrap();

        for chan in channels.into_iter() {
            if chan.update_required(&db)? {
                info!("Updating channel: {:?}", &chan);
                w.enqueue(crate::worker::WorkItem::Update {
                    chan,
                    force: false,
                    full_update: false,
                });
            }
        }
    }

    // Redirect to channel for no-javascript clicking
    Ok(Response::redirect_303("/channel/_all"))
}

/// Given a space separated list of statuses like `GE,NE`, parses each comma-separated status into actual `VideoStatus` object
fn parse_statuses(statuses: &str) -> Result<HashSet<VideoStatus>> {
    let mut ret = HashSet::new();
    let split = statuses.split(",");
    for s in split {
        let status = VideoStatus::from_str(s)?;
        ret.insert(status);
    }
    Ok(ret)
}

fn handle_response(request: &Request, workers: Arc<Mutex<WorkerPool>>) -> Response {
    if let Some(request) = request.remove_prefix("/static") {
        // Can do dynamic serving of files with:
        // return rouille::match_assets(&request, "static");

        let x = match request.url().as_ref() {
            "/popperjs_core_2.js" => Some((
                include_str!("../static/popperjs_core_2.js"),
                "application/javascript",
            )),
            "/pure-min.css" => Some((include_str!("../static/pure-min.css"), "text/css")),
            "/tippy_6.js" => Some((
                include_str!("../static/tippy_6.js"),
                "application/javascript",
            )),
            "/luxon.min.js" => Some((
                include_str!("../static/luxon.min.js"),
                "application/javascript",
            )),
            _ => None,
        };
        return match x {
            None => Response::text("404").with_status_code(404),
            Some((data, t)) => Response::from_data(t, data),
        };
    }

    let resp: Result<Response> = router!(request,
        (GET) ["/"] => {
            page_chan_list()
        },
        (GET) ["/channel/_all"] => {
            let page: i64 = request.get_param("page").and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
            let statuses = request.get_param("status").and_then(|x| parse_statuses(&x).ok());
            let filter = FilterParams {
                name_contains: request.get_param("title"),
                status: statuses,
                chanid: None,
            };
            page_list_videos(None, page, Some(filter), request.get_param("json").is_some())
        },
        (GET) ["/channel/{chanid}", chanid: i64] => {
            let page: i64 = request.get_param("page").and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
            let statuses = request.get_param("status").and_then(|x| parse_statuses(&x).ok());
            let filter = FilterParams {
                name_contains: request.get_param("title"),
                status: statuses,
                chanid: None, // TODO: Can set this to chanid and remove branching here
            };
            page_list_videos(Some(chanid), page, Some(filter), request.get_param("json").is_some())
        },
        (POST) ["/download/{videoid}", videoid: i64] => {
            page_download_video(videoid, workers.clone())
        },
        (POST) ["/ignore/{videoid}", videoid: i64] => {
            page_ignore_video(videoid)
        },

        (POST) ["/video_title/{videoid}", videoid: i64] => {
            let Some(title) = request.get_param("title") else {
                return Response::text("Missing ?title=...").with_status_code(500)
            };
            page_set_title_alt(videoid, title)
        },

        (POST) ["/video_description/{videoid}", videoid: i64] => {
            let Some(text) = request.get_param("text") else {
                return Response::text("Missing ?text=...").with_status_code(500)
            };
            page_set_description_alt(videoid, text)
        },

        (GET) ["/thumbnail/video/{id}", id: i64] => {
            page_thumbnail(id, ThumbnailType::Video, workers.clone())
        },
        (GET) ["/thumbnail/channel/{id}", id: i64] => {
            page_thumbnail(id, ThumbnailType::Channel, workers.clone())
        },
        (GET) ["/update/_all"] => {
            page_refresh(workers.clone())
        },
        // Default route
        _ => {
            Ok(Response::text("404 Not found").with_status_code(404))
        }
    );
    match resp {
        Ok(r) => r,
        Err(e) => Response::text(&format!("Internal service error: {:?}", e)).with_status_code(500),
    }
}

fn serve(workers: Arc<Mutex<WorkerPool>>) -> Result<()> {
    let cfg = Config::load();

    let addr = format!("{}:{}", cfg.web_host, cfg.web_port);
    let url = format!("http://{}", &addr);
    info!("Listening on {}", &url);
    let srv = rouille::Server::new(&addr, move |request| {
        handle_response(request, workers.clone())
    })
    .unwrap();

    let running = Arc::new(AtomicBool::new(true));

    while running.load(Ordering::SeqCst) {
        srv.poll_timeout(Duration::from_millis(100));
    }

    Ok(())
}

pub fn main() -> Result<()> {
    let workers = Arc::new(Mutex::new(crate::worker::WorkerPool::start()));

    let w = workers.clone();
    let web_thread = std::thread::spawn(|| serve(w));

    web_thread.join().unwrap()?;

    Ok(())
}
