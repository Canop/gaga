mod status;

pub use status::*;

use std::path::Path;

/// The root of the repository containing the directory, if any
pub fn find_root(dir: &Path) -> Option<&Path> {
    if !dir.is_absolute() {
        return None;
    }
    dir.ancestors().find(|dir| dir.join(".git").exists())
}
