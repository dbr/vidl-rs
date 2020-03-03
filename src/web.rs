use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use log::info;
use rouille::{router, Request, Response};
use serde_derive::Serialize;

use crate::config::Config;
use crate::db::Channel;
use crate::youtube::VideoInfo;

#[derive(Debug, Serialize)]
pub struct WebChannel {
    id: i64,
    chanid: String,
    service: String,
    title: String,
    icon: String,
}
impl From<Channel> for WebChannel {
    fn from(src: Channel) -> WebChannel {
        WebChannel {
            id: src.id,
            chanid: src.chanid,
            service: src.service.as_str().into(),
            title: src.title,
            icon: src.thumbnail,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct WebChannelList {
    channels: Vec<WebChannel>,
}

impl From<Vec<Channel>> for WebChannelList {
    fn from(src: Vec<Channel>) -> WebChannelList {
        let channels: Vec<WebChannel> = src.into_iter().map(|p| p.into()).collect();
        WebChannelList { channels }
    }
}

#[derive(Debug, Serialize)]
pub struct WebVideoInfo {
    id: String,
    title: String,
    description: String,
    thumbnail_url: String,
    published_at: String,
}

impl From<VideoInfo> for WebVideoInfo {
    fn from(src: VideoInfo) -> WebVideoInfo {
        WebVideoInfo {
            id: src.id,
            title: src.title,
            description: src.description,
            thumbnail_url: src.thumbnail_url,
            published_at: src.published_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct WebChannelVideos {
    channel: WebChannel,
    videos: Vec<WebVideoInfo>,
}

#[derive(Debug, Serialize)]
pub enum WebResponse {
    Success,
    Error(String),
}

fn web_list_channels() -> Result<Response> {
    let cfg = crate::config::Config::load();
    let db = crate::db::Database::open(&cfg)?;
    let chans = crate::db::list_channels(&db)?;
    let ret: WebChannelList = chans.into();
    Ok(Response::json(&ret))
}

fn web_channel(id: i64) -> Result<Response> {
    let cfg = crate::config::Config::load();
    let db = crate::db::Database::open(&cfg)?;

    let c = crate::db::Channel::get_by_sqlid(&db, id)?;
    let videos = c.all_videos(&db)?;

    let ret = WebChannelVideos {
        channel: c.into(),
        videos: videos.into_iter().map(|v| v.into()).collect(),
    };
    Ok(Response::json(&ret))
}

fn handle_response(request: &Request) -> Response {
    if let Some(request) = request.remove_prefix("/static") {
        if !cfg!(debug_assertions) {
            // In release mode, bundle static stuff into binary via include_str!()
            let x = match request.url().as_ref() {
                "/app.jsx" => Some((include_str!("web.rs"), "application/javascript")),
                _ => None,
            };
            return match x {
                None => Response::text("404").with_status_code(404),
                Some((data, t)) => Response::from_data(t, data),
            };
        } else {
            // In debug build, read assets from folder for reloadability
            return rouille::match_assets(&request, "static");
        }
    }

    let resp: Result<Response> = router!(request,
        (GET) ["/"] => {
            Ok(Response::html("test"))
        },
        (GET) ["/youtube/"] => {
            Ok(Response::html("test"))
        },
        (GET) ["/youtube/api/1/refresh"] => {
            Ok(Response::json(&WebResponse::Success))
        },
        (GET) ["/youtube/api/1/channels"] => {
            web_list_channels()
        },
        (GET) ["/youtube/api/1/channels/{chanid}", chanid: i64] => {
            web_channel(chanid)
        },
        /*
        (GET) ["/youtube/api/1/video/{videoid:String}/grab"] => {
            Response::html("test")
        },
        (GET) ["/youtube/api/1/video/{videoid:String}>/mark_viewed"] => {
            Response::html("test")
        },
        (GET) ["/youtube/api/1/video/{videoid:String}/mark_ignored"] => {
            Response::html("test")
        },
        (GET) ["/youtube/api/1/video_status"] => {
            Response::html("test")
        },
        (GET) ["/youtube/api/1/downloads"] => {
            Response::html("test")
        },
        (POST) ["/youtube/api/1/channel_add"] => {
            Response::html("test")
        },
        */
        // Default route
        _ => {
            Ok(Response::text("404 Not found").with_status_code(404))
        }
    );
    match resp {
        Ok(r) => r,
        Err(_) => Response::text("Internal service error").with_status_code(500),
    }
}

pub fn serve() -> Result<()> {
    let cfg = Config::load();

    println!("yep");
    let addr = format!("{}:{}", cfg.web_host, cfg.web_port);
    info!("Listening on http://{}", &addr);
    let srv = rouille::Server::new(&addr, move |request| handle_response(request)).unwrap();

    let running = Arc::new(AtomicBool::new(true));

    while running.load(Ordering::SeqCst) {
        srv.poll_timeout(Duration::from_millis(50));
    }

    Ok(())
}
