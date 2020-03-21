use anyhow::Result;

use crate::youtube::VideoInfo;

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

pub fn download(vid: &VideoInfo) -> Result<()> {
    let args: Vec<&str> = vec![&vid.url, "--newline", "-f", "18"];

    let output = Stdio::piped();
    let mut child = Command::new("youtube-dl")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args(args)
        .spawn()?;

    {
        let stdout = child
            .stdout
            .ok_or(anyhow::anyhow!("Failed to find thing"))?;

        let reader = BufReader::new(stdout);

        reader
            .lines()
            .filter_map(|line| line.ok())
            .for_each(|line| println!("{}", line));
    }

    Ok(())
}

pub fn main() -> Result<()> {
    let cfg = crate::config::Config::load();
    let db = crate::db::Database::open(&cfg)?;

    let chan_num = "1";
    // List specific channel
    let channels = crate::db::list_channels(&db)?;
    for c in channels {
        if &format!("{}", c.id) == chan_num {
            let v = c.latest_video(&db)?.unwrap();
            println!("{:?}", &v);
            download(&v.info)?;
        }
    }
    Ok(())
}
