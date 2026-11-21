use {
    crate::*,
    std::{
        fs,
        io::{
            self,
            IsTerminal,
            Write,
        },
        path::PathBuf,
    },
    toml_edit::{
        DocumentMut,
        Item,
        Table,
    },
};

/// Add what's missing in the rc files of the shell and in the
/// configuration file, and tell what was done
pub fn install() -> GagaResult<()> {
    let mut report = Vec::new();
    let mut changed = false;
    let shell = Shell::current();
    if let Some(shell) = shell {
        let line = shell.init_line();
        for (path, rc) in rc_files_to_complete(shell)? {
            let mut file = fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&path)?;
            if !rc.is_empty() && !rc.ends_with('\n') {
                writeln!(file)?;
            }
            writeln!(file, "{line}")?;
            report.push(format!("Adding {line} to {}", path.display()));
            changed = true;
        }
    } else {
        report.push("Unsupported shell: nothing added to its rc file".to_string());
    }
    let (unchanged, config_changed) = install_config(&mut report)?;
    changed |= config_changed;
    if report.is_empty() {
        println!("Nothing to change");
    } else {
        for line in report {
            println!("{line}");
        }
        if unchanged > 0 {
            println!("Other entries unchanged");
        }
    }
    println!(
        "\n{}\n\n\
        Aliases and commands are defined in {}\n\
        You can change them there, and add your own.\n\
        This list is also given by gaga --help",
        shell_help(&Config::load()?),
        Config::path()?.display(),
    );
    if changed
        && let Some(shell) = shell
        && let Some(rc_path) = shell.rc_paths()?.iter().find(|path| path.exists())
    {
        println!(
            "\nTo apply the changes, open a new shell or run\n    source {}",
            rc_path.display(),
        );
    }
    Ok(())
}

/// Offer to install when something's missing and the user
/// can be asked, print the help in other cases
pub fn install_or_help(config: Option<&Config>) -> GagaResult<()> {
    let mut actions = String::new();
    if let Some(shell) = Shell::current() {
        for (path, _) in rc_files_to_complete(shell)? {
            actions.push_str(&format!(
                "* add `{}` at the end of `{}`, to define the `gg` function, \
                its aliases, and the hook printing the status when you enter a repository\n",
                shell.init_line(),
                path.display(),
            ));
        }
    }
    let config_path = Config::path()?;
    if !config_path.exists() {
        actions.push_str(&format!(
            "* write the default configuration in `{}`\n",
            config_path.display(),
        ));
    }
    if actions.is_empty() || !io::stdin().is_terminal() {
        Args::command_with(config).print_help()?;
        return Ok(());
    }
    termimad::print_text(&format!(
        "**gaga** isn't installed yet. The installation would\n{actions}"
    ));
    print!("Install now? [Y/n] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    let read = io::stdin().read_line(&mut answer)?;
    if read == 0 || matches!(answer.trim().chars().next(), Some('n' | 'N')) {
        return Ok(());
    }
    install()
}

/// The rc files of the shell lacking the init line, with their content
fn rc_files_to_complete(shell: Shell) -> GagaResult<Vec<(PathBuf, String)>> {
    let mut paths = shell.rc_paths()?;
    if paths.iter().any(|path| path.exists()) {
        paths.retain(|path| path.exists());
    } else {
        paths.truncate(1);
    }
    let mut files = Vec::new();
    for path in paths {
        let rc = match fs::read_to_string(&path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            rc => rc?,
        };
        if !is_installed(&rc) {
            files.push((path, rc));
        }
    }
    Ok(files)
}

/// Tell whether the rc file content calls `gaga init`
fn is_installed(rc: &str) -> bool {
    rc.lines()
        .any(|line| !line.trim_start().starts_with('#') && line.contains("gaga init"))
}

/// Write the configuration file if there's none, or add to it the
/// entries of the default configuration it doesn't have.
///
/// Return the number of entries which are the default ones,
/// and whether the file was written.
fn install_config(report: &mut Vec<String>) -> GagaResult<(usize, bool)> {
    let path = Config::path()?;
    if !path.exists() {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&path, DEFAULT_CONFIG)?;
        report.push("Writing the default configuration".to_string());
        return Ok((0, true));
    }
    let toml = fs::read_to_string(&path)?;
    let (completed, unchanged) = complete_config(&toml, report).map_err(|e| match e {
        GagaError::Config { source, .. } => GagaError::Config {
            path: path.clone(),
            source,
        },
        e => e,
    })?;
    let changed = completed != toml;
    if changed {
        fs::write(&path, completed)?;
    }
    Ok((unchanged, changed))
}

/// Add to the configuration the entries of the default one it doesn't have.
///
/// Return the new configuration and the number of entries which were
/// already the default ones.
fn complete_config(
    toml: &str,
    report: &mut Vec<String>,
) -> GagaResult<(String, usize)> {
    let default_path = PathBuf::from("default-config.toml");
    let default_config = Config::parse(DEFAULT_CONFIG, default_path.clone())?;
    let defaults: toml::Table = parse_table(DEFAULT_CONFIG)?;
    let default_doc: DocumentMut = DEFAULT_CONFIG.parse()?;
    let user: toml::Table = parse_table(toml)?;
    let mut doc: DocumentMut = toml.parse()?;
    let mut unchanged = 0;
    // an entry is either a top level value, or a value of a top level table
    let mut entries = Vec::new();
    for (key, value) in &defaults {
        match value {
            toml::Value::Table(table) => {
                entries.extend(table.iter().map(|(name, value)| (Some(key), name, value)));
            }
            _ => entries.push((None, key, value)),
        }
    }
    for (section, name, default_value) in entries {
        let (label, reset) = match section.map(String::as_str) {
            Some("aliases") => {
                let target = default_value.as_str().unwrap_or_default();
                let reset = match default_config.alias_command(target) {
                    Some(tokens) => tokens.join(" "),
                    None => format!("gg {target}"),
                };
                (name.clone(), reset)
            }
            Some("commands") => {
                let command = default_config.commands.get(name);
                let reset = command.map(|command| command.command.join(" "));
                (format!("gg {name}"), reset.unwrap_or_default())
            }
            _ => {
                let reset = match default_value.as_str() {
                    Some(reset) => reset.to_string(),
                    None => default_value.to_string(),
                };
                (name.clone(), reset)
            }
        };
        let user_value = match section {
            Some(section) => user.get(section).and_then(|table| table.get(name)),
            None => user.get(name),
        };
        match user_value {
            Some(user_value) if user_value == default_value => unchanged += 1,
            Some(_) => report.push(format!(
                "Not changing your {label} definition - remove it to reset it to `{reset}`"
            )),
            None => {
                let added = match section {
                    Some(section) => {
                        let item = default_doc[section][name].clone();
                        let table = doc.entry(section).or_insert_with(|| {
                            let mut table = Table::new();
                            table.set_implicit(true);
                            Item::Table(table)
                        });
                        table
                            .as_table_like_mut()
                            .map(|table| table.insert(name, item))
                            .is_some()
                    }
                    None => {
                        doc.insert(name, default_doc[name].clone());
                        true
                    }
                };
                if added {
                    report.push(format!("Adding {label}"));
                } else {
                    report.push(format!(
                        "Not changing your {} definition",
                        section.map_or("", String::as_str),
                    ));
                }
            }
        }
    }
    Ok((doc.to_string(), unchanged))
}

fn parse_table(toml: &str) -> GagaResult<toml::Table> {
    toml::from_str(toml).map_err(|source| GagaError::Config {
        path: PathBuf::new(),
        source,
    })
}

#[test]
fn test_is_installed() {
    assert!(is_installed("alias a=b\neval \"$(gaga init zsh)\"\n"));
    assert!(!is_installed("alias a=b\n# eval \"$(gaga init zsh)\"\n"));
    assert!(!is_installed(""));
}

#[test]
fn test_complete_config() {
    let mut report = Vec::new();
    let (completed, unchanged) = complete_config(DEFAULT_CONFIG, &mut report).unwrap();
    assert_eq!(completed, DEFAULT_CONFIG);
    assert!(report.is_empty());
    assert!(unchanged > 10);

    let toml = r#"# my config
max_listed_on_enter = 20

[aliases]
gs = "status" # kept
ga = "add -p"
gl = "log"

[commands.log]
command = ["git", "log"]

[commands.rm]
command = ["trash"]
"#;
    let (completed, unchanged) = complete_config(toml, &mut report).unwrap();
    assert_eq!(unchanged, 1);
    assert!(completed.starts_with("# my config\nmax_listed_on_enter = 20\n"));
    assert!(completed.contains("gs = \"status\" # kept\nga = \"add -p\"\ngl = \"log\"\n"));
    assert!(report.contains(&"Adding gd".to_string()));
    assert!(report.contains(&"Adding jump_function".to_string()));
    assert!(report.contains(&"Adding gg add".to_string()));
    assert!(report.contains(
        &"Not changing your ga definition - remove it to reset it to `git add`".to_string()
    ));
    assert!(report.contains(
        &"Not changing your gg rm definition - remove it to reset it to `rm -r --`".to_string()
    ));
    let config = Config::parse(&completed, "test".into()).unwrap();
    assert_eq!(config.aliases["ga"], "add -p");
    assert_eq!(config.commands["rm"].command, ["trash"]);
    assert_eq!(config.commands["log"].command, ["git", "log"]);
    assert_eq!(config.commands["diff"].command, ["git", "diff"]);

    // installing again changes nothing
    let mut report = Vec::new();
    let (again, _) = complete_config(&completed, &mut report).unwrap();
    assert_eq!(again, completed);
    assert!(report.iter().all(|line| line.starts_with("Not changing")));
}
