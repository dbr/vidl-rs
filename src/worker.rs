use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use log::{debug, error, info};

use crate::{common::VideoStatus, db::DBVideoInfo};

pub enum WorkItem {
    Download(DBVideoInfo),
    Shutdown,
}

struct Worker {
    recv: Arc<Mutex<mpsc::Receiver<WorkItem>>>,
    num: usize,
}

impl Worker {
    fn run(&self) {
        loop {
            let m = self.recv.lock().unwrap().recv().unwrap();
            match m {
                WorkItem::Shutdown => {
                    info!("Shutting down worker {}", self.num);
                    return;
                }
                WorkItem::Download(ref val) => {
                    println!("Worker {}: Download {:#?}", self.num, val);
                    let cfg = crate::config::Config::load();
                    let db = crate::db::Database::open(&cfg).unwrap();

                    val.set_status(&db, VideoStatus::Downloading).unwrap();
                    let dl = crate::download::download(&val.info);

                    match dl {
                        Ok(_) => {
                            info!("Grabbed {:?} successfully", &val.info);
                            val.set_status(&db, crate::common::VideoStatus::Grabbed)
                                .unwrap()
                        }
                        Err(e) => {
                            error!("Error downloading {:?} - {:?}", &val.info, e);
                            val.set_status(&db, crate::common::VideoStatus::GrabError)
                                .unwrap();
                        }
                    };
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

    /// Stops all workers threads
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
    let v = crate::db::DBVideoInfo::get_by_sqlid(&db, 1)?;

    let p = WorkerPool::start();
    p.enqueue(WorkItem::Download(v));
    Ok(())
}