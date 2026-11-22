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

/// The file numbers designated by an argument, if it's written like
/// a number (`3`) or a range (`3-5`).
///
/// It's an error to be written like one without being valid.
pub fn number_range(arg: &OsStr) -> GagaResult<Option<RangeInclusive<usize>>> {
    let Some(arg) = arg.to_str() else {
        return Ok(None);
    };
    let (first, last) = arg.split_once('-').unwrap_or((arg, arg));
    let is_number = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !is_number(first) || !is_number(last) {
        return Ok(None);
    }
    match (first.parse(), last.parse()) {
        (Ok(first), Ok(last)) if first <= last => Ok(Some(first..=last)),
        _ => Err(GagaError::InvalidNumbers(arg.to_string())),
    }
}

/// Replace the numbers and ranges with the numbered files, which
/// are loaded only if there's a number
pub fn resolve_args<F>(
    args: &[OsString],
    load_files: F,
) -> GagaResult<Vec<OsString>>
where
    F: Fn() -> GagaResult<Vec<PathBuf>>,
{
    let mut files = None;
    let mut resolved = Vec::new();
    for arg in args {
        let Some(range) = number_range(arg)? else {
            resolved.push(arg.clone());
            continue;
        };
        if files.is_none() {
            files = Some(load_files()?);
        }
        let files = files.as_deref().unwrap_or_default();
        for number in range {
            let file = number.checked_sub(1).and_then(|idx| files.get(idx));
            resolved.push(file.ok_or(GagaError::NoFile(number))?.into());
        }
    }
    Ok(resolved)
}

#[test]
fn test_resolve_args() {
    let strings = |strs: &[&str]| strs.iter().map(OsString::from).collect::<Vec<_>>();
    let resolve = |args: &[&str]| {
        resolve_args(&strings(args), || {
            Ok(["/r/a", "/r/b", "/r/c"].iter().map(PathBuf::from).collect())
        })
    };
    assert_eq!(
        resolve(&["-p", "2", "./3", "1-2", "x-1", "2-2"]).unwrap(),
        strings(&["-p", "/r/b", "./3", "/r/a", "/r/b", "x-1", "/r/b"]),
    );
    assert!(matches!(resolve(&["2-4"]), Err(GagaError::NoFile(4))));
    assert!(matches!(resolve(&["0"]), Err(GagaError::NoFile(0))));
    for invalid in [
        "3-2",
        "99999999999999999999999",
        "1-99999999999999999999999",
    ] {
        assert!(matches!(
            resolve(&[invalid]),
            Err(GagaError::InvalidNumbers(arg)) if arg == invalid,
        ));
    }
}

#[test]
fn test_files_not_loaded_without_number() {
    let args = [OsString::from("--stat"), OsString::from("a.txt")];
    let resolved = resolve_args(&args, || Err(GagaError::NoSession)).unwrap();
    assert_eq!(resolved, args);
}
