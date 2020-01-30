extern crate env_logger;
extern crate serde;
extern crate serde_json;

use anyhow::Result;
use log::{debug, info};

#[macro_use]
extern crate serde_derive;

mod common;
mod db;
mod youtube;

fn notmain() -> Result<()> {
    let chanid = crate::common::YoutubeID{
        id: "pentadact".into(),
    };

    let yt = crate::youtube::YoutubeQuery::new(chanid.clone());
    let videos = yt.videos()?;

    let db = crate::db::Database::open()?;
    for v in videos.flatten() {
        db.insert(&v, &chanid)?;
        println!("{0}", v.title);
    }

    Ok(())
}

fn main() -> Result<()> {
    env_logger::init();
    notmain()?;
    Ok(())
}
