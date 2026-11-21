use {
    crate::*,
    rusqlite::{
        Connection,
        TransactionBehavior,
        params,
    },
    std::{
        fs,
        io,
        path::Path,
        time::{
            Duration,
            SystemTime,
            UNIX_EPOCH,
        },
    },
};

const DB_VERSION: u32 = 2;

/// The global memory of gaga, shared by all shells
pub struct Db {
    con: Connection,
}

impl Db {
    pub fn open() -> GagaResult<Self> {
        let dir = gaga_dir("XDG_STATE_HOME", ".local/state")?;
        fs::create_dir_all(&dir)?;
        Self::new(Connection::open(dir.join("gaga.db3"))?)
    }
    pub fn new(mut con: Connection) -> GagaResult<Self> {
        con.busy_timeout(Duration::from_millis(200))?;
        con.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
        if user_version(&con)? < DB_VERSION {
            let tx = con.transaction_with_behavior(TransactionBehavior::Immediate)?;
            for version in user_version(&tx)?..DB_VERSION {
                apply_patch(&tx, version)?;
            }
            tx.pragma_update(None, "user_version", DB_VERSION)?;
            tx.commit()?;
        }
        Ok(Self { con })
    }
    /// Replace the numbered files of the session,
    /// and forget the sessions whose shell is dead
    pub fn set_files(
        &mut self,
        session: u32,
        paths: &[String],
    ) -> GagaResult<()> {
        let tx = self
            .con
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let sessions = tx
            .prepare("SELECT DISTINCT session FROM session_file")?
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<u32>, _>>()?;
        for other in sessions {
            if other == session || !is_alive(other) {
                tx.execute("DELETE FROM session_file WHERE session = ?1", [other])?;
            }
        }
        for (num, path) in (1u32..).zip(paths) {
            tx.execute(
                "INSERT INTO session_file (session, num, path) VALUES (?1, ?2, ?3)",
                params![session, num, path],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    /// The numbered files of the session: number n is at index n-1
    pub fn files(
        &self,
        session: u32,
    ) -> GagaResult<Vec<String>> {
        let files = self
            .con
            .prepare("SELECT path FROM session_file WHERE session = ?1 ORDER BY num")?
            .query_map([session], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(files)
    }
    /// Remember the repository, as the last visited one
    pub fn visit(
        &self,
        root: &Path,
    ) -> GagaResult<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs() as i64);
        self.con.execute(
            "INSERT INTO repo (path, visited) VALUES (?1, ?2)
            ON CONFLICT(path) DO UPDATE SET visited = ?2",
            params![root.to_string_lossy(), now],
        )?;
        Ok(())
    }
    /// The known repositories, last visited first
    pub fn repos(&self) -> GagaResult<Vec<Repo>> {
        let repos = self
            .con
            .prepare("SELECT path FROM repo ORDER BY visited DESC, path")?
            .query_map([], |row| Ok(Repo { path: row.get(0)? }))?
            .collect::<Result<_, _>>()?;
        Ok(repos)
    }
    pub fn forget_repo(
        &self,
        repo: &Repo,
    ) -> GagaResult<()> {
        self.con
            .execute("DELETE FROM repo WHERE path = ?1", [&repo.path])?;
        Ok(())
    }
}

fn user_version(con: &Connection) -> rusqlite::Result<u32> {
    con.query_row("SELECT user_version FROM pragma_user_version", [], |row| {
        row.get(0)
    })
}

/// Bring the database from the given version to the next one
fn apply_patch(
    con: &Connection,
    version: u32,
) -> rusqlite::Result<()> {
    match version {
        0 => con.execute_batch(
            "CREATE TABLE session_file (
                session INTEGER NOT NULL,
                num INTEGER NOT NULL,
                path TEXT NOT NULL,
                PRIMARY KEY (session, num)
            )",
        ),
        1 => con.execute_batch(
            "CREATE TABLE repo (
                path TEXT PRIMARY KEY,
                visited INTEGER NOT NULL
            )",
        ),
        _ => Ok(()),
    }
}

/// Tell whether there's a process with this pid
fn is_alive(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    pid > 0
        && (unsafe { libc::kill(pid, 0) } == 0
            || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
}

#[test]
fn test_session_files() {
    let mut db = Db::new(Connection::open_in_memory().unwrap()).unwrap();
    let session = std::process::id();
    let dead_session = i32::MAX as u32;
    let paths = vec!["/r/a".to_string(), "/r/b c".to_string()];
    db.set_files(dead_session, &paths).unwrap();
    assert_eq!(db.files(dead_session).unwrap(), paths);
    db.set_files(session, &paths).unwrap();
    assert_eq!(db.files(session).unwrap(), paths);
    assert!(db.files(dead_session).unwrap().is_empty());
    db.set_files(session, &paths[1..]).unwrap();
    assert_eq!(db.files(session).unwrap(), &paths[1..]);
}

#[test]
fn test_repos() {
    let db = Db::new(Connection::open_in_memory().unwrap()).unwrap();
    db.visit(Path::new("/dev/b")).unwrap();
    db.visit(Path::new("/dev/a")).unwrap();
    db.visit(Path::new("/dev/b")).unwrap();
    let mut repos = db.repos().unwrap();
    assert_eq!(repos.len(), 2);
    db.forget_repo(&repos[0]).unwrap();
    repos.remove(0);
    assert_eq!(db.repos().unwrap(), repos);
}
