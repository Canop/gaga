use {
    crate::*,
    std::ops::RangeInclusive,
};

/// The file numbers designated by an argument like `3` or `3-5`
pub fn number_range(arg: &str) -> Option<RangeInclusive<usize>> {
    let (first, last) = arg.split_once('-').unwrap_or((arg, arg));
    let is_number = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !is_number(first) || !is_number(last) {
        return None;
    }
    Some(first.parse().ok()?..=last.parse().ok()?)
}

/// Replace the numbers and ranges with the numbered files
pub fn resolve_args(
    args: &[String],
    files: &[String],
) -> GagaResult<Vec<String>> {
    let mut resolved = Vec::new();
    for arg in args {
        match number_range(arg) {
            Some(range) => {
                for number in range {
                    let file = number.checked_sub(1).and_then(|idx| files.get(idx));
                    resolved.push(file.ok_or(GagaError::NoFile(number))?.clone());
                }
            }
            None => resolved.push(arg.clone()),
        }
    }
    Ok(resolved)
}

#[test]
fn test_resolve_args() {
    let strings = |strs: &[&str]| strs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let files = strings(&["/r/a", "/r/b", "/r/c"]);
    let resolve = |args: &[&str]| resolve_args(&strings(args), &files);
    assert_eq!(
        resolve(&["-p", "2", "./3", "1-2", "x-1"]).unwrap(),
        strings(&["-p", "/r/b", "./3", "/r/a", "/r/b", "x-1"]),
    );
    assert!(matches!(resolve(&["2-4"]), Err(GagaError::NoFile(4))));
    assert!(matches!(resolve(&["0"]), Err(GagaError::NoFile(0))));
}
