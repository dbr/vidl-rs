extern crate serde;
extern crate serde_json;

#[macro_use]
extern crate serde_derive;

mod backup;
mod cli;
mod common;
mod config;
mod db;
mod db_migration;
mod download;
mod libmig;
mod web;
mod worker;
mod youtube;

fn main() -> anyhow::Result<()> {
    Ok(cli::main()?)
}
