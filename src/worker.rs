use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use log::{debug, error, info, trace};

use crate::common::VideoStatus;
use crate::db::{Channel, DBVideoInfo};

pub enum WorkItem {
    Download(DBVideoInfo),
    Shutdown,
    UpdateCheck(Channel),
    ThumbnailCache(String),
}

struct Worker {
    recv: Arc<Mutex<mpsc::Receiver<WorkItem>>>,
    num: usize,
}

fn worker_download(val: &DBVideoInfo) -> Result<()> {
    let cfg = crate::config::Config::load();
    let db = crate::db::Database::open(&cfg)?;

    // Re-retrieve video info from DB in case it has changed since queuing
    let val = DBVideoInfo::get_by_sqlid(&db, val.id)?;

    // Only proceed with download if still `Queued`
    if val.status != VideoStatus::Queued {
        info!("Video already been downloaded, skipping - {:?}", &val);
        return Ok(());
    }

    // Mark as downloading
    val.set_status(&db, VideoStatus::Downloading)?;

    // Download
    let dl = crate::download::download(&val.info);

    match dl {
        Ok(_) => {
            info!("Grabbed {:?} successfully", &val.info);
            val.set_status(&db, crate::common::VideoStatus::Grabbed)?;
        }
        Err(e) => {
            error!("Error downloading {:?} - {:?}", &val.info, e);
            val.set_status(&db, crate::common::VideoStatus::GrabError)?;
        }
    };
    Ok(())
}

fn worker_update_check(chan: &Channel) -> Result<()> {
    let cfg = crate::config::Config::load();
    let db = crate::db::Database::open(&cfg)?;
    let last_update = chan.last_update(&db)?;
    debug!(
        "Checking channel for update {:?} - last update {:?}",
        chan, last_update
    );
    let time_to_update = if let Some(last_update) = last_update {
        let now = chrono::Utc::now();
        let delta = now - last_update;
        delta > chrono::Duration::minutes(60)
    } else {
        // No laste update, so time to update now
        true
    };

    if time_to_update {
        info!("Time to update {:?}", &chan);
        chan.update(&db)?;
    };

    Ok(())
}

fn worker_thumbnail_cache(url: &str) -> Result<()> {
    // Check if image is already in cache, as it may have been added since queued
    {
        let ic = crate::web::IMG_CACHE.lock().unwrap();
        if ic.contains(url) {
            debug!("Image already in cache, skipping");
            return Ok(());
        }
    }

    let resp = attohttpc::get(&url).send()?;
    if !resp.status().is_success() {
        error!("Failed to grab thumbnail for {}", &url);
    } else {
        let ct: String = resp
            .headers()
            .get(attohttpc::header::CONTENT_TYPE)
            .and_then(|x| x.to_str().ok())
            .unwrap_or("image/jpeg")
            .into();
        let data = resp.bytes()?;
        let img = crate::web::Image {
            content_type: ct,
            data: data,
        };
        {
            let mut ic = crate::web::IMG_CACHE.lock().unwrap();
            ic.add(&url, img);
        };
    }

    Ok(())
}

impl Worker {
    fn run(&self) {
        loop {
            let item = {
                let lock = self.recv.lock().unwrap();
                lock.recv().unwrap()

                // Drop lock
            };

            match item {
                WorkItem::Shutdown => {
                    info!("Shutting down worker {}", self.num);
                    return;
                }

                WorkItem::Download(ref val) => {
                    debug!("Worker {}: Download {:#?}", self.num, val);
                    match worker_download(val) {
                        Ok(_) => (),
                        Err(e) => error!("Error in worker {}: {:#?}", self.num, e),
                    }
                }

                WorkItem::UpdateCheck(ref chan) => {
                    debug!("Worker {}: Update check {:#?}", self.num, chan);
                    match worker_update_check(chan) {
                        Ok(_) => (),
                        Err(e) => error!("Error in worker {}: {:#?}", self.num, e),
                    }
                }

                WorkItem::ThumbnailCache(ref url) => {
                    trace!("Worker {}: Cache thumbnail {:#?}", self.num, url);
                    match worker_thumbnail_cache(url) {
                        Ok(_) => (),
                        Err(e) => error!("Error in worker {}: {:#?}", self.num, e),
                    }
                }
            }
        }
    }
}

pub struct WorkerPool {
    pool: threadpool::ThreadPool,
    num_workers: usize,
    sender: mpsc::Sender<WorkItem>,
}

impl WorkerPool {
    pub fn start() -> Self {
        let num_workers = 4;
        let pool = threadpool::ThreadPool::new(num_workers);
        let (sender, recv) = mpsc::channel();
        let recv = Arc::new(Mutex::new(recv));

        // Launch worker threads
        for curnum in 0..num_workers {
            let w = Worker {
                recv: recv.clone(),
                num: curnum,
            };
            pool.execute(move || w.run());
        }

        Self {
            pool,
            num_workers,
            sender,
        }
    }

    pub fn enqueue(&self, item: WorkItem) {
        self.sender.send(item).unwrap();
    }

    /// Completes all queued work then stops workers
    pub fn stop(self) {
        std::mem::drop(self); // Redundant as this method consumes self anyway
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        debug!("Dropping WorkerPool, starting shutdown");
        info!("Commencing worker pool shutdown");
        for _ in 0..self.num_workers {
            self.sender.send(WorkItem::Shutdown).unwrap();
        }
        debug!("Joining worker pool");
        self.pool.join();
    }
}

pub fn main() -> Result<()> {
    let cfg = crate::config::Config::load();
    let db = crate::db::Database::open(&cfg)?;

    let mut statuses = std::collections::HashSet::new();
    statuses.insert(crate::common::VideoStatus::Queued);
    let queued = crate::db::all_videos(
        &db,
        std::i64::MAX,
        0,
        Some(crate::db::FilterParams {
            name_contains: None,
            status: Some(statuses),
        }),
    )?;

    let p = WorkerPool::start();
    for q in queued {
        p.enqueue(WorkItem::Download(q));
    }

    Ok(())
}
