use {
    crate::*,
    std::{
        path::Path,
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
    pub path: String,
    pub renamed_from: Option<String>,
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
        let mut records = output.split(|&b| b == 0).map(String::from_utf8_lossy);
        while let Some(record) = records.next() {
            let Some((kind, rest)) = record.split_once(' ') else {
                continue;
            };
            match kind {
                "#" => status.read_header(rest)?,
                "1" => status.files.push(FileStatus::parse(rest, 8, None)?),
                "2" => {
                    let renamed_from = records.next().ok_or(GagaError::InvalidStatus)?;
                    let file = FileStatus::parse(rest, 9, Some(renamed_from.into_owned()))?;
                    status.files.push(file);
                }
                "u" => status.files.push(FileStatus::parse(rest, 10, None)?),
                "?" => status.files.push(FileStatus {
                    staged: '?',
                    unstaged: '?',
                    path: rest.to_string(),
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
        fields: &str,
        field_count: usize,
        renamed_from: Option<String>,
    ) -> GagaResult<Self> {
        let mut fields = fields.splitn(field_count, ' ');
        let mut states = fields.next().unwrap_or_default().chars();
        match (states.next(), states.next(), fields.nth(field_count - 2)) {
            (Some(staged), Some(unstaged), Some(path)) => Ok(Self {
                staged,
                unstaged,
                path: path.to_string(),
                renamed_from,
            }),
            _ => Err(GagaError::InvalidStatus),
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        std::{
            fs,
            path::PathBuf,
        },
    };

    fn file(
        states: &str,
        path: &str,
        renamed_from: Option<&str>,
    ) -> FileStatus {
        let mut states = states.chars();
        FileStatus {
            staged: states.next().unwrap(),
            unstaged: states.next().unwrap(),
            path: path.to_string(),
            renamed_from: renamed_from.map(String::from),
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
            ? notes/\0";
        let status = Status::parse(output).unwrap();
        assert_eq!(
            status,
            Status {
                branch: "main".to_string(),
                ahead: 2,
                behind: 1,
                files: vec![
                    file(".M", "src/a b.rs", None),
                    file("A.", "new.rs", None),
                    file("R.", "new name.rs", Some("old name.rs")),
                    file("UU", "conflict.rs", None),
                    file("??", "notes/", None),
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
            vec![file("AM", "a.txt", None), file("??", "b.txt", None)],
        );
    }
}
