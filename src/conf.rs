use {
    crate::*,
    fs_err as fs,
    lazy_regex::regex,
    serde::Deserialize,
    std::{
        collections::BTreeMap,
        env,
        ffi::OsString,
        io,
        path::PathBuf,
    },
};

/// The configuration written by the installation
pub static DEFAULT_CONFIG: &str = include_str!("../defaults/default-config.toml");

/// The content of the configuration file
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// number of changed files above which they're only counted
    /// when entering a repository
    pub max_listed_on_enter: Option<usize>,
    /// name of the shell function jumping to a repository
    pub jump_function: Option<ShellName>,
    /// shell alias -> arguments given to `gg`
    #[serde(default)]
    pub aliases: BTreeMap<ShellName, AliasTarget>,
    /// name given to `gg` -> command
    #[serde(default)]
    pub commands: BTreeMap<String, CustomCommand>,
}

/// A command of the configuration, run with `gg <name> <args>`
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomCommand {
    /// the executable and its first arguments
    pub command: Vec<String>,
    /// whether numbers and ranges in the arguments are replaced
    /// with the numbered files
    #[serde(default)]
    pub numbers: bool,
    /// whether to print the status after the command
    #[serde(default)]
    pub status_after: bool,
}

impl Config {
    /// The path of the configuration file
    pub fn path() -> GagaResult<PathBuf> {
        Ok(gaga_dir("XDG_CONFIG_HOME", ".config")?.join("config.toml"))
    }
    /// Read the configuration file, which is written by the installation
    pub fn load() -> GagaResult<Self> {
        let path = Self::path()?;
        match fs::read_to_string(&path) {
            Ok(toml) => Self::parse(&toml, path),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Err(GagaError::NoConfig),
            Err(e) => Err(e.into()),
        }
    }
    /// Parse a configuration, the path only serving to tell
    /// where an error comes from
    pub fn parse(
        toml: &str,
        path: PathBuf,
    ) -> GagaResult<Self> {
        toml::from_str(toml).map_err(|source| GagaError::Config { path, source })
    }
    /// The tokens of the command called by an alias, when it's
    /// a command of the configuration
    pub fn alias_command(
        &self,
        target: &AliasTarget,
    ) -> Option<Vec<String>> {
        let mut args = target.words();
        let mut tokens = self.commands.get(args.next()?)?.command.clone();
        tokens.extend(args.map(String::from));
        Some(tokens)
    }
}

impl CustomCommand {
    /// The tokens of the command, every `$NAME` being replaced
    /// with the content of the env variable, which must be set
    pub fn tokens(&self) -> GagaResult<Vec<OsString>> {
        let mut tokens = Vec::new();
        for token in &self.command {
            let mut expanded = OsString::new();
            let mut end = 0;
            for var in regex!(r"\$\w+").find_iter(token) {
                let name = &var.as_str()[1..];
                let value = env::var_os(name).ok_or_else(|| GagaError::NoEnvVar(name.into()))?;
                expanded.push(&token[end..var.start()]);
                expanded.push(value);
                end = var.end();
            }
            expanded.push(&token[end..]);
            tokens.push(expanded);
        }
        Ok(tokens)
    }
}

/// The gaga directory in the XDG base directory defined by the env
/// variable or, by default, by the path relative to the home
pub fn gaga_dir(
    xdg_var: &str,
    default: &str,
) -> GagaResult<PathBuf> {
    let base = match env::var_os(xdg_var).filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => home()?.join(default),
    };
    Ok(base.join("gaga"))
}

/// The home directory of the user
pub fn home() -> GagaResult<PathBuf> {
    let home = env::var_os("HOME").ok_or(GagaError::NoHome)?;
    Ok(PathBuf::from(home))
}

#[test]
fn test_default_config() {
    let config = Config::parse(DEFAULT_CONFIG, "default".into()).unwrap();
    assert!(config.commands.contains_key("add"));
    for target in config.aliases.values() {
        let name = target.words().next().unwrap();
        assert!(name == "status" || config.commands.contains_key(name));
    }
}

#[test]
fn test_tokens() {
    let command = |tokens: &[&str]| CustomCommand {
        command: tokens.iter().map(|token| token.to_string()).collect(),
        numbers: false,
        status_after: false,
    };
    let mut expanded = OsString::from("a");
    expanded.push(env::var_os("HOME").unwrap());
    expanded.push("/b");
    assert_eq!(
        command(&["a$HOME/b", "c"]).tokens().unwrap(),
        [expanded, OsString::from("c")],
    );
    assert!(matches!(
        command(&["$GAGA_UNSET_VAR"]).tokens(),
        Err(GagaError::NoEnvVar(name)) if name == "GAGA_UNSET_VAR",
    ));
}
