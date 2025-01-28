use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};
use log::{debug, info, warn};

use crate::common::{ChannelID, Service};
use crate::db;
use crate::source::base::ChannelData;
use crate::worker::{WorkItem, WorkerPool};

#[derive(Debug, Parser)]
#[clap(name = "vidl", version)]
pub(crate) struct App {
    #[clap(flatten)]
    pub(crate) global: GlobalOpts,

    #[clap(subcommand)]
    pub(crate) subcommand: Commands,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub(crate) enum CliService {
    Youtube,
    Vimeo,
}

#[derive(Debug, Args)]
pub(crate) struct GlobalOpts {
    /// Verbosity level (can be specified multiple times)
    #[clap(long, short, global = true, action = clap::ArgAction::Count)]
    pub(crate) verbose: u8,
}

#[derive(Debug, Args)]
pub(crate) struct CmdAdd {
    pub(crate) chanid: String,
    /// youtube or vimeo
    #[clap(value_enum, default_value_t=CliService::Youtube)]
    pub(crate) service: CliService,
}

#[derive(Debug, Args)]
pub(crate) struct CmdRemove {
    pub(crate) id: i64,
}

#[derive(Debug, Args)]
pub(crate) struct CmdList {
    pub(crate) id: Option<i64>,
}

#[derive(Debug, Args)]
pub(crate) struct CmdUpdate {
    /// Checks for new data even if already updated recently
    #[clap(long, short)]
    pub(crate) force: bool,
    /// Checks all pages, instead of stopping on an previously-seen video
    #[clap(long)]
    pub(crate) full_update: bool,
    /// Filter by channel name
    #[clap()]
    pub(crate) filter: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub(crate) struct CmdBackupExport {
    /// Output file
    #[clap(short, long)]
    output: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub(crate) struct CmdBackupImport {}

#[derive(Debug, Subcommand, Clone)]
pub(crate) enum CmdBackupOpts {
    Export(CmdBackupExport),
    Import(CmdBackupImport),
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    /// Add channel
    Add(CmdAdd),

    /// Backup database as simple .json file
    #[clap(subcommand)]
    Backup(CmdBackupOpts),

    /// enqueues videos for download
    Download,

    /// Initialise the database
    Init,

    /// list channels/videos
    List(CmdList),

    /// update database schema to be current
    Migrate,

    /// remove given channel and all videos in it
    Remove(CmdRemove),

    /// Updates all added channel info
    Update(CmdUpdate),

    /// serve web interface
    Web,

    /// downloads queued videos
    Worker,
}

fn update(force: bool, full_update: bool, filter: Option<String>) -> Result<()> {
    // Load config
    debug!("Loading config");
    let cfg = crate::config::Config::load();
    let db = db::Database::open(&cfg)?;

    let work = WorkerPool::start();

    // Get list of channels
    let channels = db::list_channels(&db)?;
    if channels.is_empty() {
        warn!("No channels yet added");
    }

    // Queue update
    for chan in channels.into_iter() {
        if let Some(f) = &filter {
            let matched = chan.title.to_lowercase().contains(&f.to_lowercase());
            if !matched {
                continue;
            }
        }

        if force || chan.update_required(&db)? {
            info!("Updating channel: {:?}", &chan);
            work.enqueue(WorkItem::Update {
                chan,
                force,
                full_update,
            });
        }
    }

    // Wait for queue to empty
    work.stop();

    Ok(())
}

/// Add channel
fn add(name: &str, service_str: &str) -> Result<()> {
    let service = Service::from_str(service_str)?;
    let cid = crate::source::invidious::find_channel_id(name, &service)?;

    match &cid {
        ChannelID::Youtube(ytid) => {
            let yt = crate::source::invidious::YoutubeQuery::new(&ytid);

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
fn remove(chan_num: i64) -> Result<()> {
    let cfg = crate::config::Config::load();
    let db = db::Database::open(&cfg)?;

    let chan = db::Channel::get_by_sqlid(&db, chan_num)?;

    info!("Removing channel {:?}", &chan);
    chan.delete(&db)?;

    Ok(())
}

/// List videos
fn list(chan_num: Option<i64>) -> Result<()> {
    let cfg = crate::config::Config::load();
    let db = db::Database::open(&cfg)?;

    if let Some(chan_num) = chan_num {
        // List specific channel
        let channels = db::list_channels(&db)?;
        for c in channels {
            if c.id == chan_num {
                for v in c.all_videos(&db, 50, 0, None)? {
                    let v = v.info;
                    let title_alt = if let Some(a) = v.title_alt {
                        format!(" {}", a)
                    } else {
                        "".to_string()
                    };
                    println!(
                        "ID: {}\nTitle: {}{}\nURL: {}\nPublished: {}\nThumbnail: {}\nDescription: {}\n----",
                        v.id, v.title, title_alt, v.url, v.published_at, v.thumbnail_url, v.description
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
    let args = App::parse();

    config_logging(args.global.verbose as u64)?;

    match args.subcommand {
        Commands::Add(o) => {
            add(
                &o.chanid,
                match o.service {
                    CliService::Youtube => "youtube",
                    CliService::Vimeo => "vimeo",
                },
            )?;
        }
        Commands::Backup(o) => match o {
            CmdBackupOpts::Export(o) => {
                crate::backup::export(o.output.as_deref())?;
            }
            CmdBackupOpts::Import(_) => {
                crate::backup::import()?;
            }
        },
        Commands::Download => {
            todo!()
        }
        Commands::Init => {
            init()?;
        }
        Commands::List(o) => {
            list(o.id)?;
        }
        Commands::Migrate => {
            migrate()?;
        }
        Commands::Remove(o) => {
            remove(o.id)?;
        }
        Commands::Update(o) => {
            update(o.force, o.full_update, o.filter)?;
        }
        Commands::Web => {
            crate::web::main()?;
        }
        Commands::Worker => {
            crate::worker::main()?;
        }
    }

    Ok(())
}
