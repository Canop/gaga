use {
    crate::*,
    clap::ValueEnum,
    std::{
        env,
        fmt::Write,
        path::{
            Path,
            PathBuf,
        },
    },
    unicode_width::UnicodeWidthStr,
};

/// A shell for which gaga can generate its functions and hook
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Zsh,
}

impl Shell {
    /// The shell of the user, when it's a supported one
    pub fn current() -> Option<Self> {
        let shell = env::var_os("SHELL")?;
        match Path::new(&shell).file_name()?.to_str()? {
            "bash" => Some(Self::Bash),
            "zsh" => Some(Self::Zsh),
            _ => None,
        }
    }
    /// The line to have in the rc files
    pub fn init_line(self) -> String {
        format!("eval \"$(gaga init {})\"", self.name())
    }
    /// The name of the shell, as given to `gaga init`
    pub fn name(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
        }
    }
    /// The files evaluated when an interactive shell starts,
    /// the first one being the one to create if there's none
    pub fn rc_paths(self) -> GagaResult<Vec<PathBuf>> {
        match self {
            Self::Bash => {
                let home = home()?;
                Ok(vec![home.join(".bashrc"), home.join(".bash_profile")])
            }
            Self::Zsh => {
                let dir = match env::var_os("ZDOTDIR").filter(|dir| !dir.is_empty()) {
                    Some(dir) => PathBuf::from(dir),
                    None => home()?,
                };
                Ok(vec![dir.join(".zshrc")])
            }
        }
    }
}

/// The function calling gaga with the id of the shell session,
/// and with `bare` when there's no argument
const GG_FUNCTION: &str = r#"unalias gg 2>/dev/null
function gg {
    GAGA_SESSION=$$ gaga "${@:-bare}"
}
"#;

/// Call `gaga enter` when the directory changes
const ZSH_HOOK: &str = r#"
function _gaga_chpwd {
    GAGA_SESSION=$$ gaga enter "$OLDPWD"
}
autoload -Uz add-zsh-hook
add-zsh-hook chpwd _gaga_chpwd
"#;

/// Call `gaga enter` before a prompt if the directory changed since
/// the previous one, while keeping the exit code of the last command
const BASH_HOOK: &str = r#"
function _gaga_prompt {
    local code=$?
    if [ "$PWD" != "$_gaga_pwd" ]; then
        _gaga_pwd=$PWD
        GAGA_SESSION=$$ gaga enter "$OLDPWD"
    fi
    return $code
}
_gaga_pwd=$PWD
case "$PROMPT_COMMAND" in
    *_gaga_prompt*) ;;
    *) PROMPT_COMMAND="_gaga_prompt${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;;
esac
"#;

/// Build the code to be evaluated by the shell on start
pub fn init_script(
    shell: Shell,
    config: &Config,
) -> String {
    let mut script = GG_FUNCTION.to_string();
    for (alias, target) in &config.aliases {
        let _ = writeln!(script, "alias {alias}='gg {target}'");
    }
    if let Some(name) = &config.jump_function {
        script.push_str(&jump_script(shell, name));
    }
    script.push_str(match shell {
        Shell::Bash => BASH_HOOK,
        Shell::Zsh => ZSH_HOOK,
    });
    script
}

/// List the shell functions and aliases defined by the init script
pub fn shell_help(config: &Config) -> String {
    let jump_function = config.jump_function.as_ref().map(ShellName::as_str);
    let width = jump_function.map_or(2, |name| name.width().max(2));
    let mut help = format!("Shell functions:\n  {}  gaga", padded("gg", width));
    if let Some(bare) = config.commands.get("bare") {
        let _ = write!(help, ", or without argument: {}", bare.command.join(" "));
    }
    if let Some(name) = jump_function {
        let name = padded(name, width);
        let _ = write!(help, "\n  {name}  jump to a visited repository");
    }
    let aliases = config
        .aliases
        .iter()
        .map(|(alias, target)| (alias.as_str(), target.as_str(), target));
    let alias_width = aliases.clone().map(|(alias, _, _)| alias.width()).max();
    let target_width = aliases.clone().map(|(_, target, _)| target.width()).max();
    if let (Some(alias_width), Some(target_width)) = (alias_width, target_width) {
        help.push_str("\n\nAliases:");
        for (alias, target, alias_target) in aliases {
            let tokens = config.alias_command(alias_target).unwrap_or_default();
            let line = format!(
                "\n  {}  gg {}  {}",
                padded(alias, alias_width),
                padded(target, target_width),
                tokens.join(" "),
            );
            help.push_str(line.trim_end());
        }
    }
    help
}

/// The string, followed by the spaces needed to fill the given
/// number of terminal columns
fn padded(
    s: &str,
    width: usize,
) -> String {
    let missing = width.saturating_sub(s.width());
    format!("{s}{:missing$}", "")
}

#[test]
fn test_shell_help_with_wide_chars() {
    let config = Config::parse(
        "jump_function = \"\u{65e5}\u{672c}\u{8a9e}\"\n\
        [aliases]\n\
        \"g\u{65e5}\" = \"status\"\n\
        \"\u{e9}\" = \"add\"\n\
        [commands.add]\n\
        command = [\"git\", \"add\"]\n",
        "test".into(),
    )
    .unwrap();
    assert_eq!(
        shell_help(&config),
        "Shell functions:\n  \
        gg      gaga\n  \
        \u{65e5}\u{672c}\u{8a9e}  jump to a visited repository\n\
        \n\
        Aliases:\n  \
        g\u{65e5}  gg status\n  \
        \u{e9}    gg add     git add",
    );
}

/// Build the function jumping to a repository, and its completion
fn jump_script(
    shell: Shell,
    name: &ShellName,
) -> String {
    let function = format!(
        r#"unalias {name} 2>/dev/null
function {name} {{
    if [ $# -eq 0 ]; then
        gaga repos
    else
        local dir
        dir=$(gaga jump "$1") && cd "$dir"
    fi
}}
"#
    );
    let completion = match shell {
        Shell::Bash => format!(
            r#"function _gaga_repos {{
    local IFS=$'\n'
    COMPREPLY=($(compgen -W "$(gaga repos)" -- "${{COMP_WORDS[COMP_CWORD]}}"))
}}
complete -F _gaga_repos {name}
"#
        ),
        Shell::Zsh => format!(
            r#"function _gaga_repos {{
    compadd -- ${{(f)"$(gaga repos)"}}
}}
if (( $+functions[compdef] )); then
    compdef _gaga_repos {name}
fi
"#
        ),
    };
    function + &completion
}
