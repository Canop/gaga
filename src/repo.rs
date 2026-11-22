use std::{
    borrow::Cow,
    path::PathBuf,
};

/// A repository of the global memory
#[derive(Debug, PartialEq)]
pub struct Repo {
    /// path of the root
    pub path: PathBuf,
}

impl Repo {
    /// The name, for display and matching
    pub fn name(&self) -> Cow<'_, str> {
        self.path
            .file_name()
            .unwrap_or(self.path.as_os_str())
            .to_string_lossy()
    }
    pub fn exists(&self) -> bool {
        self.path.join(".git").exists()
    }
}

/// Find the first repository whose name is the given one or,
/// if there's none, starts with it or, if there's none, contains it
pub fn best_match<'r>(
    repos: &'r [Repo],
    name: &str,
) -> Option<&'r Repo> {
    let find =
        |matches: fn(&str, &str) -> bool| repos.iter().find(|repo| matches(&repo.name(), name));
    find(|repo_name, name| repo_name == name)
        .or_else(|| find(|repo_name, name| repo_name.starts_with(name)))
        .or_else(|| find(|repo_name, name| repo_name.contains(name)))
}

#[test]
fn test_best_match() {
    let repos: Vec<Repo> = [
        "/dev/broot-web",
        "/dev/broot",
        "/old/broot",
        "/dev/termimad",
    ]
    .into_iter()
    .map(|path| Repo {
        path: PathBuf::from(path),
    })
    .collect();
    let check = |name, expected: Option<&str>| {
        assert_eq!(
            best_match(&repos, name).map(|repo| repo.path.as_path()),
            expected.map(std::path::Path::new),
        );
    };
    check("broot", Some("/dev/broot"));
    check("bro", Some("/dev/broot-web"));
    check("mad", Some("/dev/termimad"));
    check("bacon", None);
}
