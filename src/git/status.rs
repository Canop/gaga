use {
    crate::*,
    std::{
        ffi::OsStr,
        os::unix::ffi::OsStrExt,
        path::{
            Path,
            PathBuf,
        },
        process::Command,
    },
};

#[derive(Debug, Default, PartialEq)]
pub struct Status {
    /// name of the branch, or short id of the commit when HEAD is detached
    pub branch: String,
    pub ahead: usize,
    pub behind: usize,
    pub files: Vec<FileStatus>,
}

#[derive(Debug, PartialEq)]
pub struct FileStatus {
    /// state in the index, `.` when unchanged
    pub staged: char,
    /// state in the work tree, `.` when unchanged
    pub unstaged: char,
    /// path relative to the root of the repository
    pub path: PathBuf,
    pub renamed_from: Option<PathBuf>,
}

impl Status {
    pub fn read(dir: &Path) -> GagaResult<Self> {
        let output = Command::new("git")
            .args([
                "--no-optional-locks",
                "status",
                "--porcelain=v2",
                "--branch",
                "-z",
            ])
            .current_dir(dir)
            .output()
            .map_err(|source| GagaError::Spawn {
                exe: "git".to_string(),
                source,
            })?;
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(GagaError::Git(error.trim().to_string()));
        }
        Self::parse(&output.stdout)
    }
    /// Parse the output of `git status --porcelain=v2 --branch -z`
    pub fn parse(output: &[u8]) -> GagaResult<Self> {
        let mut status = Self::default();
        let mut records = output.split(|&b| b == 0);
        while let Some(record) = records.next() {
            let Some(space_idx) = record.iter().position(|&b| b == b' ') else {
                continue;
            };
            let (kind, rest) = (&record[..space_idx], &record[space_idx + 1..]);
            match kind {
                b"#" => status.read_header(&String::from_utf8_lossy(rest))?,
                b"1" => status.files.push(FileStatus::parse(rest, 8, None)?),
                b"2" => {
                    let renamed_from = records.next().ok_or(GagaError::InvalidStatus)?;
                    let file = FileStatus::parse(rest, 9, Some(path_from_bytes(renamed_from)))?;
                    status.files.push(file);
                }
                b"u" => status.files.push(FileStatus::parse(rest, 10, None)?),
                b"?" => status.files.push(FileStatus {
                    staged: '?',
                    unstaged: '?',
                    path: path_from_bytes(rest),
                    renamed_from: None,
                }),
                _ => {}
            }
        }
        Ok(status)
    }
    fn read_header(
        &mut self,
        header: &str,
    ) -> GagaResult<()> {
        match header.split_once(' ') {
            Some(("branch.oid", oid)) => {
                self.branch = oid.chars().take(7).collect();
            }
            Some(("branch.head", head)) if head != "(detached)" => {
                self.branch = head.to_string();
            }
            Some(("branch.ab", ab)) => {
                let counts = ab.split_once(' ').and_then(|(ahead, behind)| {
                    let ahead = ahead.strip_prefix('+')?.parse().ok()?;
                    let behind = behind.strip_prefix('-')?.parse().ok()?;
                    Some((ahead, behind))
                });
                (self.ahead, self.behind) = counts.ok_or(GagaError::InvalidStatus)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl FileStatus {
    /// Parse the fields of a changed entry: the first one holds
    /// the two states, the last one is the path
    fn parse(
        fields: &[u8],
        field_count: usize,
        renamed_from: Option<PathBuf>,
    ) -> GagaResult<Self> {
        let mut fields = fields.splitn(field_count, |&b| b == b' ');
        match (fields.next(), fields.nth(field_count - 2)) {
            (Some(&[staged, unstaged]), Some(path)) => Ok(Self {
                staged: staged as char,
                unstaged: unstaged as char,
                path: path_from_bytes(path),
                renamed_from,
            }),
            _ => Err(GagaError::InvalidStatus),
        }
    }
}

fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    PathBuf::from(OsStr::from_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        std::fs,
    };

    fn file(
        states: &str,
        path: &[u8],
        renamed_from: Option<&[u8]>,
    ) -> FileStatus {
        let mut states = states.chars();
        FileStatus {
            staged: states.next().unwrap(),
            unstaged: states.next().unwrap(),
            path: path_from_bytes(path),
            renamed_from: renamed_from.map(path_from_bytes),
        }
    }

    #[test]
    fn test_parse() {
        let output = b"# branch.oid 0123456789abcdef\0\
            # branch.head main\0\
            # branch.upstream origin/main\0\
            # branch.ab +2 -1\0\
            1 .M N... 100644 100644 100644 aaaa bbbb src/a b.rs\0\
            1 A. N... 000000 100644 100644 0000 bbbb new.rs\0\
            2 R. N... 100644 100644 100644 aaaa bbbb R100 new name.rs\0old name.rs\0\
            u UU N... 100644 100644 100644 100644 aaaa bbbb cccc conflict.rs\0\
            ? notes/\0\
            ? not\xffutf8\0";
        let status = Status::parse(output).unwrap();
        assert_eq!(
            status,
            Status {
                branch: "main".to_string(),
                ahead: 2,
                behind: 1,
                files: vec![
                    file(".M", b"src/a b.rs", None),
                    file("A.", b"new.rs", None),
                    file("R.", b"new name.rs", Some(b"old name.rs")),
                    file("UU", b"conflict.rs", None),
                    file("??", b"notes/", None),
                    file("??", b"not\xffutf8", None),
                ],
            }
        );
    }

    #[test]
    fn test_parse_detached() {
        let output = b"# branch.oid 0123456789abcdef\0# branch.head (detached)\0";
        let status = Status::parse(output).unwrap();
        assert_eq!(status.branch, "0123456");
    }

    struct TestRepo {
        dir: PathBuf,
    }
    impl TestRepo {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("gaga-{name}-{}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            let repo = Self { dir };
            repo.git(&["init", "-q", "-b", "trunk"]);
            repo
        }
        fn git(
            &self,
            args: &[&str],
        ) {
            let status = Command::new("git")
                .args(args)
                .current_dir(&self.dir)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .status()
                .unwrap();
            assert!(status.success());
        }
    }
    impl Drop for TestRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn test_read() {
        let repo = TestRepo::new("read");
        fs::write(repo.dir.join("a.txt"), "a").unwrap();
        fs::write(repo.dir.join("b.txt"), "b").unwrap();
        repo.git(&["add", "a.txt"]);
        fs::write(repo.dir.join("a.txt"), "aa").unwrap();
        let status = Status::read(&repo.dir).unwrap();
        assert_eq!(status.branch, "trunk");
        assert_eq!(
            status.files,
            vec![file("AM", b"a.txt", None), file("??", b"b.txt", None)],
        );
    }

    // macOS refuses such file names
    #[cfg(target_os = "linux")]
    #[test]
    fn test_read_non_utf8() {
        let repo = TestRepo::new("non-utf8");
        fs::write(repo.dir.join(path_from_bytes(b"not\xffutf8")), "a").unwrap();
        let status = Status::read(&repo.dir).unwrap();
        assert_eq!(status.files, vec![file("??", b"not\xffutf8", None)]);
    }
}
