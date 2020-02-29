use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use log::info;
use rouille::{router, Request, Response};
use serde_derive::Serialize;

use crate::config::Config;

#[derive(Debug, Serialize)]
pub enum WebResponse {
    Success,
    Error(String),
}

fn handle_response(request: &Request) -> Response {
    if let Some(request) = request.remove_prefix("/static") {
        if !cfg!(debug_assertions) {
            // In release mode, bundle static stuff into binary via include_str!
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

    router!(request,
        (GET) ["/"] => {
            Response::html("test")
        },
        (GET) ["/youtube/"] => {
            Response::html("test")
        },
        (GET) ["/youtube/api/1/refresh"] => {
            Response::json(&WebResponse::Success)
        },
        (GET) ["/youtube/api/1/channels"] => {
            Response::html("test")
        },
        (GET) ["/youtube/api/1/channels/{chanid:String}"] => {
            Response::html("test")
        },
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
        // Default route
        _ => {
            Response::text("404 Not found").with_status_code(404)
        }
    )
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
