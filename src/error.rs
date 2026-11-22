use {
    std::{
        io::{
            self,
            IsTerminal,
            Write,
        },
        path::PathBuf,
        process::ExitCode,
    },
    termimad::{
        CompoundStyle,
        MadSkin,
        crossterm::style::{
            Attribute,
            Color,
        },
    },
};

/// Messages are markdown, rendered by [`GagaError::report`]
#[derive(thiserror::Error, Debug)]
pub enum GagaError {
    #[error("**Error:** {0}")]
    Io(#[from] io::Error),
    #[error("**Database error:** {0}")]
    Db(#[from] rusqlite::Error),
    #[error("**Error:** the database was written by a more recent version of gaga")]
    DbTooRecent,
    #[error("**Error:** invalid configuration in `{path}`: {source}")]
    Config {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("**Error:** {0}")]
    TomlEdit(#[from] toml_edit::TomlError),
    #[error("**Error:** no configuration file: run `gaga install`")]
    NoConfig,
    #[error("**Error:** can't read the current directory: {0}")]
    Cwd(#[source] io::Error),
    #[error("**Error:** no `HOME` in env")]
    NoHome,
    #[error("**Error:** not in a git repository")]
    NotInRepo,
    #[error("**Error:** numbers can only be used through the `gg` shell function")]
    NoSession,
    #[error("**Error:** no file with number `{0}`")]
    NoFile(usize),
    #[error("**Error:** no known repository matches `{0}`")]
    NoRepo(String),
    #[error("**Error:** unknown command `{0}`")]
    UnknownCommand(String),
    #[error("**Error:** env variable `{0}` isn't set")]
    NoEnvVar(String),
    #[error("**Error:** nothing to run for command `{0}`")]
    EmptyCommand(String),
    #[error("**Error:** can't run `{exe}`: {source}")]
    Spawn { exe: String, source: io::Error },
    #[error("**Error:** git status failed: {0}")]
    Git(String),
    #[error("**Error:** unexpected output of git status")]
    InvalidStatus,
    /// The launched command already told why
    #[error("command failed")]
    CommandFailed(u8),
}

pub type GagaResult<T> = Result<T, GagaError>;

impl GagaError {
    pub fn report(&self) -> ExitCode {
        match self {
            Self::Io(e) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::from(141),
            Self::CommandFailed(code) => ExitCode::from(*code),
            _ => {
                let skin = if io::stderr().is_terminal() {
                    MadSkin {
                        bold: CompoundStyle::new(Some(Color::Red), None, Attribute::Bold.into()),
                        inline_code: CompoundStyle::with_fg(Color::Yellow),
                        ..Default::default()
                    }
                } else {
                    MadSkin::no_style()
                };
                let mut stderr = io::stderr().lock();
                for line in self.to_string().lines() {
                    if writeln!(stderr, "{}", skin.inline(line)).is_err() {
                        break;
                    }
                }
                ExitCode::FAILURE
            }
        }
    }
}
