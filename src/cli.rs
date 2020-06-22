use anyhow::Result;
use clap::{App, Arg, SubCommand};
use log::{debug, info, warn};

use crate::common::{ChannelID, Service};
use crate::db;
use crate::worker::{WorkItem, WorkerPool};

fn update() -> Result<()> {
    // Load config
    debug!("Loading config");
    let cfg = crate::config::Config::load();
    let db = db::Database::open(&cfg)?;

    let work = WorkerPool::start();

    // Get list of channels
    let channels = db::list_channels(&db)?;
    if channels.len() == 0 {
        warn!("No channels yet added");
    }

    // Queue update
    for chan in channels.into_iter() {
        info!("Updating channel: {:?}", &chan);
        work.enqueue(WorkItem::UpdateCheck(chan));
    }

    // Wait for queue to empty
    work.stop();

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
            let db = db::Database::open(&cfg)?;
            info!("Adding Youtube channel {:?}", &ytid.id,);
            db::Channel::create(&db, &cid, &meta.title, &meta.thumbnail)?;
            Ok(())
        }
        ChannelID::Vimeo(_) => Err(anyhow::anyhow!("Not yet implemented")),
    }
}

/// Remove channel and videos
fn remove(chan_num: Option<&str>) -> Result<()> {
    let cfg = crate::config::Config::load();
    let db = db::Database::open(&cfg)?;

    let id = chan_num.unwrap().parse()?;

    let chan = db::Channel::get_by_sqlid(&db, id)?;

    info!("Removing channel {:?}", &chan);
    chan.delete(&db)?;

    Ok(())
}

/// List videos
fn list(chan_num: Option<&str>) -> Result<()> {
    let cfg = crate::config::Config::load();
    let db = db::Database::open(&cfg)?;

    if let Some(chan_num) = chan_num {
        // List specific channel
        let channels = db::list_channels(&db)?;
        for c in channels {
            if format!("{}", c.id) == chan_num {
                for v in c.all_videos(&db, 50, 0, None)? {
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
        let channels = db::list_channels(&db)?;
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

fn migrate() -> Result<()> {
    let cfg = crate::config::Config::load();
    db::Database::migrate(&cfg)?;
    Ok(())
}

fn init() -> Result<()> {
    let cfg = crate::config::Config::load();
    db::Database::create(&cfg)?;
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

pub fn main() -> Result<()> {
    // Init DB subcommand
    let sc_init = SubCommand::with_name("init").about("Initialise the database");

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

    let sc_remove = SubCommand::with_name("remove")
        .about("remove given channel and all videos in it")
        .arg(Arg::with_name("id").required(true));

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
    let sc_worker = SubCommand::with_name("worker").about("downloads queued videos");

    // DB update command
    let sc_migrate = SubCommand::with_name("migrate").about("update database schema to be current");

    // Main command
    let app = App::new("vidl")
        .subcommand(sc_init)
        .subcommand(sc_add)
        .subcommand(sc_remove)
        .subcommand(sc_update)
        .subcommand(sc_list)
        .subcommand(sc_web)
        .subcommand(sc_backup)
        .subcommand(sc_download)
        .subcommand(sc_worker)
        .subcommand(sc_migrate)
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
        ("remove", Some(sub_m)) => remove(sub_m.value_of("id"))?,
        ("update", Some(_sub_m)) => update()?,
        ("list", Some(sub_m)) => list(sub_m.value_of("id"))?,
        ("web", Some(_sub_m)) => crate::web::main()?,
        ("backup", Some(sub_m)) => match sub_m.subcommand() {
            ("export", Some(sub_m)) => crate::backup::export(sub_m.value_of("output"))?,
            ("import", Some(_sub_m)) => crate::backup::import()?,
            _ => return Err(anyhow::anyhow!("Unhandled backup subcommand")),
        },
        ("worker", Some(_sub_m)) => crate::worker::main()?,
        ("init", Some(_sub_m)) => init()?,
        ("migrate", Some(_sub_m)) => migrate()?,
        _ => {
            return Err(anyhow::anyhow!("Unhandled subcommand"));
        }
    };

    Ok(())
}
