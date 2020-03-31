extern crate serde;
extern crate serde_json;

use anyhow::Result;
use log::{debug, error, info, warn};

#[macro_use]
extern crate serde_derive;

use clap::{App, Arg, SubCommand};

mod backup;
mod common;
mod config;
mod db;
mod download;
mod web;
mod worker;
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
        let last_update = chan.last_update(&db)?;
        let needs_update = if let Some(last_update) = last_update {
            let now = chrono::Utc::now();
            let delta = now - last_update;
            let interval = chrono::Duration::hours(1);
            delta > interval
        } else {
            // Not yet been updated
            true
        };

        // Skip if no updated required
        if !needs_update {
            info!("Channel updated recently, skipping {:?}", &chan);
            continue;
        }

        info!("Updating channel: {:?}", &chan);

        // Set updated time now, even in case of failure
        chan.set_last_update(&db)?;

        if chan.service.as_str() != "youtube" {
            // FIXME
            error!("Ignoring Vimeo channel {:?}", &chan);
            continue;
        }
        let chanid = crate::common::YoutubeID {
            id: chan.chanid.clone(),
        };

        let yt = crate::youtube::YoutubeQuery::new(&chanid);
        let meta = yt.get_metadata();

        match meta {
            Ok(meta) => chan.update_metadata(&db, &meta)?,
            Err(e) => {
                error!(
                    "Error fetching metadata for {:?} - {} - skipping channel",
                    chanid, e
                );
                // Skip to next channel
                continue;
            }
        }

        let videos = yt.videos();

        let newest_video = chan.latest_video(&db)?;

        debug!(
            "Oldest video for channel {:?} is {:?}",
            &chan, &newest_video
        );

        let mut new_videos: Vec<crate::youtube::VideoInfo> = vec![];
        for v in videos {
            let v = v?;

            if let Some(ref newest) = newest_video {
                if v.url == newest.info.url || v.published_at <= newest.info.published_at {
                    // Stop adding videos once we've seen one as-new
                    debug!("Already seen video Video {:?}", &v);
                    break;
                }
            }
            new_videos.push(v);
        }

        for v in new_videos {
            debug!("Adding {0}", v.title);
            match chan.add_video(&db, &v) {
                Ok(_) => (),
                Err(e) => error!("Error adding video {:?} - {:?}", &v, e),
            };
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
                for v in c.all_videos(&db, 50, 0)? {
                    let v = v.info;
                    println!(
                        "ID: {}\nTitle: {}\nURL: {}\nPublished: {}\nThumbnail: {}\nDescription: {}\n----",
                        v.id, v.title, v.url, v.published_at, v.thumbnail_url, v.description
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
        .level_for("vidl", internal_level)
        .chain(std::io::stdout())
        .apply()?;

    Ok(())
}

fn main() -> Result<()> {
    // Add channel subcommand
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

    // Update subcommand
    let sc_update = SubCommand::with_name("update").about("Updates all added channel info");

    // List subcommand
    let sc_list = SubCommand::with_name("list")
        .about("list channels/videos")
        .arg(Arg::with_name("id"));

    // Web subcommand
    let sc_web = SubCommand::with_name("web").about("serve web interface");

    // Backup subcommands
    let sc_import = SubCommand::with_name("import").about("import DB backup");
    let sc_export = SubCommand::with_name("export")
        .about("export DB backup")
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("output")
                .takes_value(true),
        );
    let sc_backup = SubCommand::with_name("backup")
        .about("Backup database as simple .json file")
        .subcommand(sc_import)
        .subcommand(sc_export);

    // Download subcommand
    let sc_download = SubCommand::with_name("download").about("enqueues videos for download");

    // Download subcommand
    let sc_worker = SubCommand::with_name("worker").about("download worker thread test");

    // Main command
    let app = App::new("vidl")
        .subcommand(sc_add)
        .subcommand(sc_update)
        .subcommand(sc_list)
        .subcommand(sc_web)
        .subcommand(sc_backup)
        .subcommand(sc_download)
        .subcommand(sc_worker)
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .multiple(true)
                .takes_value(false)
                .global(true),
        );

    // Parse
    let app_m = app.get_matches();

    // Logging levels
    let verbosity = app_m.occurrences_of("verbose");
    config_logging(verbosity)?;

    match app_m.subcommand() {
        ("add", Some(sub_m)) => add(
            sub_m
                .value_of("chanid")
                .expect("required arg chanid missing"),
            sub_m
                .value_of("service")
                .expect("required arg service missing"),
        )?,
        ("update", Some(_sub_m)) => update()?,
        ("list", Some(sub_m)) => list(sub_m.value_of("id"))?,
        ("web", Some(_sub_m)) => crate::web::main()?,
        ("backup", Some(sub_m)) => match sub_m.subcommand() {
            ("export", Some(sub_m)) => crate::backup::export(sub_m.value_of("output"))?,
            ("import", Some(_sub_m)) => crate::backup::import()?,
            _ => return Err(anyhow::anyhow!("Unhandled backup subcommand")),
        },
        ("download", Some(_sub_m)) => crate::download::main()?,
        ("worker", Some(_sub_m)) => crate::worker::main()?,
        _ => {
            return Err(anyhow::anyhow!("Unhandled subcommand"));
        }
    };

    Ok(())
}
