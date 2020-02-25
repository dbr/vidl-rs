extern crate env_logger;
extern crate serde;
extern crate serde_json;

use anyhow::Result;
use log::{debug, info, warn};

#[macro_use]
extern crate serde_derive;

use clap::{App, Arg, SubCommand};

mod common;
mod config;
mod db;
mod youtube;

use crate::common::{ChannelID, Service};

fn update() -> Result<()> {
    // Load config
    debug!("Loading config");
    let cfg = crate::config::Config::load();
    let db = crate::db::Database::open(&cfg)?;

    let channels = crate::db::list_channels(&db)?;
    if channels.len() == 0 {
        warn!("No channels yet added");
    }
    for chan in channels.iter() {
        info!("Updating channel: {:?}", &chan);

        assert_eq!(chan.service.as_str(), "youtube"); // FIXME
        let chanid = crate::common::YoutubeID {
            id: chan.chanid.clone(),
        };

        let yt = crate::youtube::YoutubeQuery::new(&chanid);
        let videos = yt.videos()?;

        let newest_video = chan.latest_video(&db)?;
        for v in videos.flatten() {
            if let Some(ref newest) = newest_video {
                if v.published_at <= newest.published_at {
                    // Stop adding videos once we've seen one as-new
                    debug!("Video {:?} already seen", &v);
                    break;
                }
            }
            chan.add_video(&db, &v)?;
            debug!("Added {0}", v.title);
        }
    }

    Ok(())
}

/// Add channel
fn add(name: &str, service_str: &str) -> Result<()> {
    let service = Service::from_str(service_str)?;
    let cid = crate::youtube::find_channel_id(name, &service)?;

    match &cid {
        ChannelID::Youtube(ytid) => {
            let yt = crate::youtube::YoutubeQuery::new(&ytid);

            let meta = yt.get_metadata()?;
            let cfg = crate::config::Config::load();
            let db = crate::db::Database::open(&cfg)?;
            info!("Adding Youtube channel {:?}", &ytid.id,);
            db::Channel::create(&db, &cid, &meta.title, &meta.thumbnail)?;
            Ok(())
        }
        ChannelID::Vimeo(_) => Err(anyhow::anyhow!("Not yet implemented")),
    }
}

/// List videos
fn list(chan_num: Option<&str>) -> Result<()> {
    let cfg = crate::config::Config::load();
    let db = crate::db::Database::open(&cfg)?;

    if let Some(chan_num) = chan_num {
        // List specific channel
        let channels = crate::db::list_channels(&db)?;
        for c in channels {
            if &format!("{}", c.id) == chan_num {
                for v in c.all_videos(&db)? {
                    println!(
                        "Title: {}\nPublished: {}\nThumbnail: {}\nDescription: {}\n----",
                        v.title, v.published_at, v.thumbnail_url, v.description
                    );
                }
            }
        }
    } else {
        // List all channels
        let channels = crate::db::list_channels(&db)?;
        for c in channels {
            println!(
                "{} - {} ({} on service {})\nThumbnail: {}",
                c.id,
                c.title,
                c.chanid,
                c.service.as_str(),
                c.thumbnail,
            );
        }
    }
    Ok(())
}

fn config_logging(verbosity: u64) -> Result<()> {
    // Level for this application
    let internal_level = match verbosity {
        0 => log::LevelFilter::Warn,
        1 => log::LevelFilter::Info,  // -v
        2 => log::LevelFilter::Debug, // -vv
        _ => log::LevelFilter::Trace, // -vvv
    };

    // Show log output for 3rd party library at -vvv
    let thirdparty_level = match verbosity {
        0 => log::LevelFilter::Warn,
        1 => log::LevelFilter::Warn,  // -v
        2 => log::LevelFilter::Warn,  // -vv
        _ => log::LevelFilter::Debug, // -vvv
    };

    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        .level(thirdparty_level)
        .level_for("ytdl", internal_level)
        .chain(std::io::stdout())
        .apply()?;

    Ok(())
}

fn main() -> Result<()> {
    let sc_add = SubCommand::with_name("add")
        .about("Add channel")
        .arg(Arg::with_name("chanid").required(true))
        .arg(
            Arg::with_name("service")
                .required(true)
                .default_value("youtube")
                .possible_values(&["youtube", "vimeo"])
                .value_name("youtube|vimeo"),
        );
    let sc_update = SubCommand::with_name("update").about("Updates all added channel info");

    let sc_list = SubCommand::with_name("list")
        .about("list channels/videos")
        .arg(Arg::with_name("id"));

    let app = App::new("ytdl")
        .subcommand(sc_add)
        .subcommand(sc_update)
        .subcommand(sc_list)
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .multiple(true)
                .takes_value(false)
                .global(true),
        );

    let app_m = app.get_matches();

    // Logging levels
    let verbosity = app_m.occurrences_of("verbose");
    config_logging(verbosity)?;

    match app_m.subcommand() {
        ("add", Some(sub_m)) => add(
            sub_m.value_of("chanid").unwrap(),
            sub_m.value_of("service").unwrap(),
        )?,
        ("update", Some(_sub_m)) => update()?,
        ("list", Some(sub_m)) => list(sub_m.value_of("id"))?,
        _ => {
            eprintln!("Error: Unknown subcommand");
        }
    };

    Ok(())
}
