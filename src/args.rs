use {
    crate::*,
    clap::{
        CommandFactory,
        Parser,
        Subcommand,
        ValueEnum,
    },
    std::{
        ffi::OsString,
        io::{
            self,
            IsTerminal,
        },
        path::PathBuf,
    },
};

#[derive(Debug, Parser)]
#[command(about, version)]
pub struct Args {
    /// whether to have styles and colors
    #[arg(long, default_value = "auto", value_name = "color")]
    pub color: TriBool,

    #[command(subcommand)]
    pub action: Option<Action>,
}

#[derive(Debug, Subcommand)]
pub enum Action {
    /// add what's missing in the rc file of the shell and in the configuration
    Install,

    /// print the shell code defining gg, the aliases and the hook
    Init { shell: Shell },

    /// print the status, with numbered files
    #[command(visible_alias = "s")]
    Status,

    /// print the names of the known repositories, last visited first
    Repos,

    /// print the path of the known repository best matching the name
    Jump { name: String },

    /// print the status unless the previous directory was in the same repository
    #[command(hide = true)]
    Enter { previous: PathBuf },

    /// a command defined in the configuration
    #[command(external_subcommand)]
    Custom(Vec<OsString>),
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriBool {
    Auto,
    Yes,
    No,
}

impl Args {
    /// The clap command, whose help lists the shell functions
    /// and aliases when the configuration is known
    pub fn command_with(config: Option<&Config>) -> clap::Command {
        let command = Self::command();
        match config {
            Some(config) => command.after_help(shell_help(config)),
            None => command,
        }
    }
    pub fn color(&self) -> bool {
        match self.color {
            TriBool::Auto => io::stdout().is_terminal(),
            TriBool::Yes => true,
            TriBool::No => false,
        }
    }
}
