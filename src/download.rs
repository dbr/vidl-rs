use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use log::debug;

use crate::config::Config;
use crate::source::base::VideoInfo;

pub fn download(vid: &VideoInfo) -> Result<()> {
    let cfg = Config::load();

    // Ensure output folder exists
    std::fs::create_dir_all(&cfg.download_dir).context("Failed to make output folder")?;

    let output_template = &cfg.download_dir.join(cfg.filename_format);

    // Prepare command arguments
    let mut args: Vec<&str> = vec![];

    // First option required by progress parser
    args.push("--newline");
    args.push("--output");
    args.push(output_template.to_str().unwrap());

    // Then options from config
    args.extend(
        cfg.extra_youtubedl_args
            .iter()
            .map(|x: &String| -> &str { x.as_ref() }),
    );

    // Final arg is video URL
    args.push(&vid.url);

    debug!("Running youtube-dl with args {:#?}", args);

    let mut child = Command::new("youtube-dl")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args(args)
        .spawn()?;

    {
        let stdout = child
            .stdout
            .take()
            .ok_or(anyhow::anyhow!("Failed to find thing"))?;

        let reader = BufReader::new(stdout);

        let stderr = child
            .stderr
            .take()
            .ok_or(anyhow::anyhow!("Failed to find thing"))?;
        let reader_err = BufReader::new(stderr);

        reader
            .lines()
            .filter_map(|line| line.ok())
            .for_each(|line| println!("{}", line));

        reader_err
            .lines()
            .filter_map(|line| line.ok())
            .for_each(|line| println!("ERR: {}", line));
    }
    let exit = child.wait()?;
    if !exit.success() {
        return Err(anyhow::anyhow!(
            "youtube-dl exited with non-zero exit status {}",
            exit
        ));
    }

    Ok(())
}
