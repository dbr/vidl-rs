use crate::libmig::{Migration, Migrator};

#[derive(Debug)]
struct CreateBase;

impl Migration for CreateBase {
    fn get_name(&self) -> &str {
        "create initial channel and video tables"
    }
    fn get_version(&self) -> i64 {
        1
    }

    fn up(&self, conn: &rusqlite::Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE channel (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                chanid        TEXT NOT NULL,
                service       TEXT NOT NULL,
                title         TEXT NOT NULL,
                thumbnail     TEXT NOT NULL,
                last_update   DATETIME NULL
            );
            CREATE TABLE video (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                channel       INTEGER NOT NULL,
                video_id      TEXT NOT NULL,
                status        TEXT NOT NULL,
                url           TEXT NOT NULL UNIQUE,
                title         TEXT NOT NULL,
                description   TEXT NOT NULL,
                thumbnail     TEXT NOT NULL,
                published_at  DATETIME NOT NULL,
                FOREIGN KEY(channel) REFERENCES channel(id)
            );
  
            CREATE INDEX idx_video_published_at ON video (
                published_at
            );
            CREATE INDEX idx_video_channel ON video (
                channel
            );
            ",
        )
        .map(|_| ())
    }
}

#[derive(Debug)]
struct AddDuration;

impl Migration for AddDuration {
    fn get_name(&self) -> &str {
        "Add duration to videos"
    }
    fn get_version(&self) -> i64 {
        2
    }

    fn up(&self, conn: &rusqlite::Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "
            ALTER TABLE video
            ADD COLUMN duration INTEGER NOT NULL DEFAULT (0)
            ",
        )
        .map(|_| ())
    }
}

#[derive(Debug)]
struct M03AddInsertionDate;

impl Migration for M03AddInsertionDate {
    fn get_name(&self) -> &str {
        "Add date_added field to videos"
    }
    fn get_version(&self) -> i64 {
        3
    }

    fn up(&self, conn: &rusqlite::Connection) -> rusqlite::Result<()> {
        // Add column with null value
        conn.execute_batch(
            "
            ALTER TABLE video
            ADD COLUMN date_added DATETIME DEFAULT CURRENT_TIMESTAMP
            ",
        )
        .map(|_| ())?;

        Ok(())
    }
}

#[derive(Debug)]
struct M04AddAltTitle;

impl Migration for M04AddAltTitle {
    fn get_name(&self) -> &str {
        "Add title_alt to videos"
    }
    fn get_version(&self) -> i64 {
        4
    }

    fn up(&self, conn: &rusqlite::Connection) -> rusqlite::Result<()> {
        // Add column with null value
        conn.execute_batch(
            "
            ALTER TABLE video
            ADD COLUMN title_alt TEXT
            ",
        )
        .map(|_| ())?;

        Ok(())
    }
}

#[derive(Debug)]
struct M05AddAltDescription;

impl Migration for M05AddAltDescription {
    fn get_name(&self) -> &str {
        "Add description_alt to videos"
    }
    fn get_version(&self) -> i64 {
        5
    }

    fn up(&self, conn: &rusqlite::Connection) -> rusqlite::Result<()> {
        // Add column with null value
        conn.execute_batch(
            "
            ALTER TABLE video
            ADD COLUMN description_alt TEXT
            ",
        )
        .map(|_| ())?;

        Ok(())
    }
}
pub fn get_migrator(db: &rusqlite::Connection) -> Migrator {
    Migrator {
        migs: vec![
            Box::new(CreateBase {}),
            Box::new(AddDuration {}),
            Box::new(M03AddInsertionDate {}),
            Box::new(M04AddAltTitle {}),
            Box::new(M05AddAltDescription {}),
        ],
        db: &db,
    }
}
