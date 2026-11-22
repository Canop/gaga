use {
    crate::*,
    std::{
        ffi::{
            OsStr,
            OsString,
        },
        ops::RangeInclusive,
        path::PathBuf,
    },
};

/// The file numbers designated by an argument like `3` or `3-5`
pub fn number_range(arg: &OsStr) -> Option<RangeInclusive<usize>> {
    let arg = arg.to_str()?;
    let (first, last) = arg.split_once('-').unwrap_or((arg, arg));
    let is_number = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !is_number(first) || !is_number(last) {
        return None;
    }
    Some(first.parse().ok()?..=last.parse().ok()?)
}

/// Replace the numbers and ranges with the numbered files
pub fn resolve_args(
    args: &[OsString],
    files: &[PathBuf],
) -> GagaResult<Vec<OsString>> {
    let mut resolved = Vec::new();
    for arg in args {
        match number_range(arg) {
            Some(range) => {
                for number in range {
                    let file = number.checked_sub(1).and_then(|idx| files.get(idx));
                    resolved.push(file.ok_or(GagaError::NoFile(number))?.into());
                }
            }
            None => resolved.push(arg.clone()),
        }
    }
    Ok(resolved)
}

#[test]
fn test_resolve_args() {
    let strings = |strs: &[&str]| strs.iter().map(OsString::from).collect::<Vec<_>>();
    let files: Vec<PathBuf> = ["/r/a", "/r/b", "/r/c"].iter().map(PathBuf::from).collect();
    let resolve = |args: &[&str]| resolve_args(&strings(args), &files);
    assert_eq!(
        resolve(&["-p", "2", "./3", "1-2", "x-1"]).unwrap(),
        strings(&["-p", "/r/b", "./3", "/r/a", "/r/b", "x-1"]),
    );
    assert!(matches!(resolve(&["2-4"]), Err(GagaError::NoFile(4))));
    assert!(matches!(resolve(&["0"]), Err(GagaError::NoFile(0))));
}
