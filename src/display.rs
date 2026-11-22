use {
    crate::*,
    std::{
        fmt::Display,
        io::{
            self,
            Write,
        },
        os::unix::ffi::OsStrExt,
        path::{
            Path,
            PathBuf,
        },
    },
    termimad::crossterm::style::{
        Color,
        ContentStyle,
        StyledContent,
    },
};

static BRANCH_COLOR: u8 = 172;
static NUMBER_COLOR: u8 = 244;
static STAGED_COLOR: u8 = 71;
static UNSTAGED_COLOR: u8 = 209;

/// How the changed files are shown
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileListing {
    Count,
    List,
    NumberedList,
}

pub struct StatusPrinter<'a> {
    pub color: bool,
    pub listing: FileListing,
    pub cwd: &'a Path,
    pub root: &'a Path,
}

impl StatusPrinter<'_> {
    pub fn write<W: Write>(
        &self,
        w: &mut W,
        status: &Status,
    ) -> io::Result<()> {
        write!(w, "{}", self.paint(sanitize(&status.branch), BRANCH_COLOR))?;
        if status.ahead > 0 {
            write!(w, " ↑{}", status.ahead)?;
        }
        if status.behind > 0 {
            write!(w, " ↓{}", status.behind)?;
        }
        if self.listing == FileListing::Count {
            return writeln!(w, " - {} changed files", status.files.len());
        }
        writeln!(w)?;
        let width = status.files.len().to_string().len();
        for (idx, file) in status.files.iter().enumerate() {
            if self.listing == FileListing::NumberedList {
                let number = format!("{:>width$}", idx + 1);
                write!(w, "{} ", self.paint(number, NUMBER_COLOR))?;
            }
            let staged_color = match file.staged {
                '?' => UNSTAGED_COLOR,
                _ => STAGED_COLOR,
            };
            write!(
                w,
                "{}{} ",
                self.paint(state_char(file.staged), staged_color),
                self.paint(state_char(file.unstaged), UNSTAGED_COLOR),
            )?;
            if let Some(renamed_from) = &file.renamed_from {
                write!(w, "{} -> ", self.display_path(renamed_from))?;
            }
            writeln!(w, "{}", self.display_path(&file.path))?;
        }
        Ok(())
    }
    fn paint<D: Display>(
        &self,
        content: D,
        ansi_color: u8,
    ) -> StyledContent<D> {
        let mut style = ContentStyle::new();
        if self.color {
            style.foreground_color = Some(Color::AnsiValue(ansi_color));
        }
        style.apply(content)
    }
    /// The path as seen from the current directory, in a
    /// form which is only good for display
    fn display_path(
        &self,
        path: &Path,
    ) -> String {
        let relative = relative_path(&self.root.join(path), self.cwd);
        let mut relative = relative.to_string_lossy().into_owned();
        if path.as_os_str().as_bytes().ends_with(b"/") && !relative.ends_with('/') {
            relative.push('/');
        }
        sanitize(&relative)
    }
}

fn state_char(state: char) -> char {
    match state {
        '.' => ' ',
        _ => state,
    }
}

/// The path, relative to the base (both must be absolute)
pub fn relative_path(
    path: &Path,
    base: &Path,
) -> PathBuf {
    let mut path_parts = path.components().peekable();
    let mut base_parts = base.components().peekable();
    while let (Some(a), Some(b)) = (path_parts.peek(), base_parts.peek()) {
        if a != b {
            break;
        }
        path_parts.next();
        base_parts.next();
    }
    let mut relative = PathBuf::new();
    for _ in base_parts {
        relative.push("..");
    }
    relative.extend(path_parts);
    if relative.as_os_str().is_empty() {
        relative.push(".");
    }
    relative
}

/// Escape the control characters, which could harm the terminal
pub fn sanitize(s: &str) -> String {
    let mut sanitized = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_control() {
            sanitized.extend(c.escape_default());
        } else {
            sanitized.push(c);
        }
    }
    sanitized
}

#[test]
fn test_relative_path() {
    let check = |path, base, expected| {
        assert_eq!(
            relative_path(Path::new(path), Path::new(base)),
            Path::new(expected),
        );
    };
    check("/r/src/a.rs", "/r", "src/a.rs");
    check("/r/a.rs", "/r/src/cli", "../../a.rs");
    check("/r/src/a.rs", "/r/src", "a.rs");
    check("/r/src", "/r/src", ".");
}

#[test]
fn test_write_status() {
    let status = Status::parse(
        b"# branch.oid 0123456789abcdef\0\
        # branch.head main\0\
        # branch.ab +2 -0\0\
        1 .M N... 100644 100644 100644 aaaa bbbb src/a.rs\0\
        2 R. N... 100644 100644 100644 aaaa bbbb R100 b.rs\0src/old\x1b.rs\0\
        ? notes/\0",
    )
    .unwrap();
    let mut printer = StatusPrinter {
        color: false,
        listing: FileListing::NumberedList,
        cwd: Path::new("/r/src"),
        root: Path::new("/r"),
    };
    let mut out = Vec::new();
    printer.write(&mut out, &status).unwrap();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "main ↑2\n\
        1  M a.rs\n\
        2 R  old\\u{1b}.rs -> ../b.rs\n\
        3 ?? ../notes/\n",
    );
    printer.listing = FileListing::List;
    let mut out = Vec::new();
    printer.write(&mut out, &status).unwrap();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "main ↑2\n\
        \x20M a.rs\n\
        R  old\\u{1b}.rs -> ../b.rs\n\
        ?? ../notes/\n",
    );
    printer.listing = FileListing::Count;
    let mut out = Vec::new();
    printer.write(&mut out, &status).unwrap();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "main ↑2 - 3 changed files\n"
    );
}
