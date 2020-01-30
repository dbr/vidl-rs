extern crate env_logger;
extern crate serde;
extern crate serde_json;

use anyhow::Result;
use log::{debug, info};

#[macro_use]
extern crate serde_derive;

mod db;
mod youtube;

fn notmain() -> Result<()> {
    let chanid = "pentadact";
    info!("Querying channel {}", &chanid);
    let yt = crate::youtube::YoutubeQuery::new(chanid.into());
    let videos = yt.videos()?;

    let db = crate::db::Database::open()?;
    for v in videos.flatten() {
        if v.title.contains("Leverage") {
            return Ok(());
        }
        db.insert(&v)?;
        println!("{0}", v.title);
    }

    Ok(())
}

fn main() -> Result<()> {
    env_logger::init();
    notmain()?;
    Ok(())
}
