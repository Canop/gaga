use std::path::Path;

/// A repository of the global memory
#[derive(Debug, PartialEq)]
pub struct Repo {
    /// path of the root
    pub path: String,
}

impl Repo {
    pub fn name(&self) -> &str {
        Path::new(&self.path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&self.path)
    }
    pub fn exists(&self) -> bool {
        Path::new(&self.path).join(".git").exists()
    }
}

/// Find the first repository whose name is the given one or,
/// if there's none, starts with it or, if there's none, contains it
pub fn best_match<'r>(
    repos: &'r [Repo],
    name: &str,
) -> Option<&'r Repo> {
    let find =
        |matches: fn(&str, &str) -> bool| repos.iter().find(|repo| matches(repo.name(), name));
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
        path: path.to_string(),
    })
    .collect();
    let check = |name, expected: Option<&str>| {
        assert_eq!(
            best_match(&repos, name).map(|repo| repo.path.as_str()),
            expected
        );
    };
    check("broot", Some("/dev/broot"));
    check("bro", Some("/dev/broot-web"));
    check("mad", Some("/dev/termimad"));
    check("bacon", None);
}
