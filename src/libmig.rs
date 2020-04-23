//! A greatly simplified migration system.
//!
//! Only supports upgrading schema versions - downgrades are expected to be
//! handled by reverting to a backup. Reasons being:
//! - VIDL isn't exactly critical infrastructure. Almost all info can be
//!   recreated from scratch (except the video status).
//! - Migrations will most often be used to add columns, which are difficult to
//!   write downgrade queries for (as there is no `DELETE COLUMN` in SQLite)
use rusqlite::params;
use rusqlite::OptionalExtension;

pub trait Migration: std::fmt::Debug {
    fn get_version(&self) -> i64;
    fn get_name(&self) -> &str;
    fn up(&self, conn: &rusqlite::Connection) -> rusqlite::Result<()>;
}
pub struct Migrator<'a> {
    pub migs: Vec<Box<dyn Migration>>,
    pub db: &'a rusqlite::Connection,
}

impl<'a> Migrator<'a> {
    /// Create the meta-table
    pub fn setup(&self) -> anyhow::Result<()> {
        self.db.execute(
            "CREATE TABLE IF NOT EXISTS vidl_migration (
                key STRING PRIMARY KEY UNIQUE,
                value INTEGER
            )",
            params![],
        )?;
        Ok(())
    }

    /// Store schema version in database
    fn set_db_version(&self, version: i64) -> anyhow::Result<()> {
        println!("Setting version to {}", version);
        self.db.execute(
            r#"
            INSERT OR REPLACE INTO vidl_migration(key, value)
            VALUES("current_version", ?1);"#,
            params![version],
        )?;
        Ok(())
    }

    /// Get the schema version from the DB
    pub fn get_db_version(&self) -> anyhow::Result<Option<i64>> {
        let ver: Option<i64> = self
            .db
            .query_row(
                r#"SELECT value FROM vidl_migration WHERE key = "current_version" LIMIT 1"#,
                params![],
                |row| row.get(0),
            )
            .optional()?;
        Ok(ver)
    }

    /// Get the latest migration's version
    pub fn get_latest_version(&self) -> i64 {
        self.migs.iter().map(|x| x.get_version()).max().unwrap()
    }

    fn to_version(&self, target: i64) -> anyhow::Result<()> {
        // FIXME: Return more specific errors.
        println!("Moving to version {}", target);

        let cur_ver = self.get_db_version()?;

        if let Some(cur_ver) = cur_ver {
            if cur_ver > target {
                return Err(anyhow::anyhow!("Cannot migrate backwards"));
            }
        }
        let mut sorted = self
            .migs
            .iter()
            .map(|x| Box::new(x.as_ref()))
            .collect::<Vec<Box<&dyn Migration>>>();
        sorted.sort_by_cached_key(|x| x.get_version());
        dbg!(&sorted);
        let to_perform = sorted
            .iter()
            .skip_while(|x| x.get_version() <= cur_ver.unwrap_or(std::i64::MIN))
            .take_while(|x| x.get_version() <= target)
            .map(|x| Box::new(**x))
            .collect::<Vec<Box<&dyn Migration>>>();

        println!("To perform: {:?}", &to_perform);

        for m in to_perform {
            println!("Perform up of {:?}", &m);
            m.up(&self.db)?;
            self.set_db_version(m.get_version())?;
        }

        Ok(())
    }

    pub fn is_db_current(&self) -> anyhow::Result<bool> {
        let is_cur = if let Some(cur_ver) = self.get_db_version()? {
            cur_ver == self.get_latest_version()
        } else {
            false
        };
        Ok(is_cur)
    }

    pub fn upgrade(&self) -> anyhow::Result<()> {
        let db_ver = self.get_db_version()?;
        let latest = self.get_latest_version();

        if let Some(db_ver) = db_ver {
            if db_ver == latest {
                // Already up to date
                return Err(anyhow::anyhow!(
                    "Already on latest database schema version ({})",
                    db_ver
                ));
            }

            if db_ver > latest {
                // Futuristic DB (runing old version of VIDL)
                return Err(anyhow::anyhow!(
                    "Database schema version ({}) is newer than this version of VIDL supports ({})",
                    db_ver,
                    latest,
                ));
            }
        }

        self.to_version(self.get_latest_version())?;
        Ok(())
    }
}

#[test]
fn test_migration() {
    #[derive(Debug)]
    struct CreateBase {}
    impl Migration for CreateBase {
        fn get_version(&self) -> i64 {
            1
        }
        fn get_name(&self) -> &str {
            "Create base"
        }
        fn up(&self, conn: &rusqlite::Connection) -> rusqlite::Result<()> {
            conn.execute(
                "
                CREATE TABLE video (
                    id            INTEGER PRIMARY KEY AUTOINCREMENT,
                    title         TEXT NOT NULL
                );
                ",
                params![],
            )
            .map(|_| ())
        }
    }
    #[derive(Debug)]
    struct AddColumn {}

    impl Migration for AddColumn {
        fn get_version(&self) -> i64 {
            2
        }
        fn get_name(&self) -> &str {
            "Add channels"
        }
        fn up(&self, conn: &rusqlite::Connection) -> rusqlite::Result<()> {
            conn.execute(
                "
                CREATE TABLE channels (
                    id INTEGER PRIMARY KEY AUTOINCREMENT
                );
                ",
                params![],
            )
            .map(|_| ())
        }
    }

    #[derive(Debug)]
    struct RemoveChannel {}

    impl Migration for RemoveChannel {
        fn get_version(&self) -> i64 {
            3
        }
        fn get_name(&self) -> &str {
            "Remove channel"
        }
        fn up(&self, conn: &rusqlite::Connection) -> rusqlite::Result<()> {
            conn.execute(
                "
                DROP table channels;
                ",
                params![],
            )
            .map(|_| ())
        }
    }
    // Test migrations ^

    let db = crate::db::Database::create_in_memory().unwrap();

    let mig = Migrator {
        migs: vec![
            Box::new(CreateBase {}),
            Box::new(AddColumn {}),
            Box::new(RemoveChannel {}),
        ],
        db: &db.conn,
    };
    mig.setup().unwrap();

    // No DB version yet
    assert_eq!(mig.get_db_version().unwrap(), None);
    // Three migrations exists
    assert_eq!(mig.get_latest_version(), 3);

    // Move to version 1
    println!("Test: Moving to version 1");
    mig.to_version(1).unwrap();
    assert_eq!(mig.get_db_version().unwrap(), Some(1));

    // Then to verison 2
    println!("Test: Moving to version 2");
    mig.to_version(2).unwrap();
    assert_eq!(mig.get_db_version().unwrap(), Some(2));

    // Back down to 1 should error
    println!("Test: Back to version 1");
    assert!(mig.to_version(1).is_err());
    // ...with no change to schema version
    assert_eq!(mig.get_db_version().unwrap(), Some(2));

    // Then to latest
    println!("Test: Moving to latest (3)");
    mig.upgrade().unwrap();
    assert_eq!(mig.get_db_version().unwrap(), Some(3));
}
