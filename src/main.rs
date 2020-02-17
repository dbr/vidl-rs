extern crate env_logger;
extern crate serde;
extern crate serde_json;

use anyhow::Result;
use log::{debug, info};

#[macro_use]
extern crate serde_derive;

use clap::{App, Arg, SubCommand};

mod common;
mod db;
mod youtube;

fn update() -> Result<()> {
    let db = crate::db::Database::open()?;

    let channels = crate::db::list_channels(&db)?;
    for chan in channels {
        info!("Updating channel: {:?}", &chan);

        assert_eq!(chan.service.as_str(), "youtube");
        let chanid = crate::common::YoutubeID {
            id: chan.chanid.clone(),
        };

        let yt = crate::youtube::YoutubeQuery::new(chanid.clone());
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

fn add(chanid: &str, service_str: &str) -> Result<()> {
    let db = crate::db::Database::open()?;
    let service = crate::db::Service::from_str(service_str)?;
    info!("Adding channel {} from service {:?}", &chanid, &service);
    db::Channel::get_or_create(&db, chanid, service)?;
    Ok(())
}

fn config_logging(verbosity: u64) -> Result<()> {
    // Level for this application
    let internal_level = match verbosity {
        0 => log::LevelFilter::Error,
        1 => log::LevelFilter::Info,  // -v
        2 => log::LevelFilter::Debug, // -vv
        _ => log::LevelFilter::Trace, // -vvv
    };

    // Show log output for 3rd party library at -vvv
    let thirdparty_level = match verbosity {
        0 => log::LevelFilter::Error,
        1 => log::LevelFilter::Error, // -v
        2 => log::LevelFilter::Error, // -vv
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
        .arg(Arg::with_name("service").required(true));
    let sc_update = SubCommand::with_name("update").about("Updates all added channel info");

    let app = App::new("ytdl")
        .subcommand(sc_add)
        .subcommand(sc_update)
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .multiple(true)
                .takes_value(false),
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
        _ => {
            eprintln!("Error: Unknown subcommand");
        }
    };

    Ok(())
}
