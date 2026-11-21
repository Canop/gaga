use {
    crate::*,
    lazy_regex::regex_replace_all,
    serde::Deserialize,
    std::{
        collections::BTreeMap,
        env,
        fs,
        io,
        path::PathBuf,
    },
};

pub static DEFAULT_CONFIG: &str = include_str!("../defaults/default-config.toml");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub max_listed_on_enter: Option<usize>,
    /// name of the shell function jumping to a repository
    pub jump_function: Option<String>,
    /// shell alias -> arguments given to `gg`
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
    #[serde(default)]
    pub commands: BTreeMap<String, CustomCommand>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomCommand {
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
        target: &str,
    ) -> Option<Vec<String>> {
        let mut args = target.split_whitespace();
        let mut tokens = self.commands.get(args.next()?)?.command.clone();
        tokens.extend(args.map(String::from));
        Some(tokens)
    }
}

impl CustomCommand {
    /// The tokens of the command, every `$NAME` being replaced
    /// with the content of the env variable
    pub fn tokens(&self) -> Vec<String> {
        self.command
            .iter()
            .map(|token| {
                regex_replace_all!(r"\$(\w+)", token, |_, name| {
                    env::var(name).unwrap_or_default()
                })
                .into_owned()
            })
            .collect()
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

pub fn home() -> GagaResult<PathBuf> {
    let home = env::var_os("HOME").ok_or(GagaError::NoHome)?;
    Ok(PathBuf::from(home))
}

#[test]
fn test_default_config() {
    let config = Config::parse(DEFAULT_CONFIG, "default".into()).unwrap();
    assert!(config.commands.contains_key("add"));
    for target in config.aliases.values() {
        let name = target.split_whitespace().next().unwrap();
        assert!(name == "status" || config.commands.contains_key(name));
    }
}
