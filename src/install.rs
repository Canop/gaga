use {
    crate::*,
    fs_err as fs,
    std::{
        io::{
            self,
            IsTerminal,
            Write,
        },
        path::{
            Path,
            PathBuf,
        },
    },
    termimad::{
        CompoundStyle,
        MadSkin,
        crossterm::style::{
            Attribute,
            Color::AnsiValue,
        },
    },
    toml_edit::{
        DocumentMut,
        Item,
        Table,
    },
};

/// Add what's missing in the rc files of the shell and in the
/// configuration file, and tell what was done
pub fn install<W: Write>(
    w: &mut W,
    color: bool,
) -> GagaResult<()> {
    // report lines are markdown
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
            report.push(format!("Adding `{line}` to `{}`", path.display()));
            changed = true;
        }
    } else {
        report.push("Unsupported shell: nothing added to its rc file".to_string());
    }
    let (unchanged, config_changed) = install_config(&mut report)?;
    changed |= config_changed;
    if report.is_empty() {
        writeln!(w, "Nothing to change")?;
    } else {
        let skin = if color {
            MadSkin {
                bold: CompoundStyle::new(Some(AnsiValue(172)), None, Attribute::Bold.into()),
                inline_code: CompoundStyle::with_fg(AnsiValue(244)),
                ..Default::default()
            }
        } else {
            MadSkin::no_style()
        };
        for line in report {
            writeln!(w, "{}", skin.inline(&line))?;
        }
        if unchanged > 0 {
            writeln!(w, "Other entries unchanged")?;
        }
    }
    writeln!(
        w,
        "\n{}\n\n\
        Aliases and commands are defined in {}\n\
        You can change them there, and add your own.\n\
        This list is also given by gaga --help",
        shell_help(&Config::load()?),
        Config::path()?.display(),
    )?;
    if changed
        && let Some(shell) = shell
        && let Some(rc_path) = shell.rc_paths()?.iter().find(|path| path.exists())
    {
        writeln!(
            w,
            "\nTo apply the changes, open a new shell or run\n    source {}",
            rc_path.display(),
        )?;
    }
    Ok(())
}

/// Offer to install when something's missing and the user
/// can be asked, print the help in other cases
pub fn install_or_help<W: Write>(
    w: &mut W,
    color: bool,
    config: Option<&Config>,
) -> GagaResult<()> {
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
    let offer = format!("**gaga** isn't installed yet. The installation would\n{actions}");
    write!(w, "{}", termimad::term_text(&offer))?;
    write!(w, "Install now? [Y/n] ")?;
    w.flush()?;
    let mut answer = String::new();
    let read = io::stdin().read_line(&mut answer)?;
    if read == 0 || matches!(answer.trim().chars().next(), Some('n' | 'N')) {
        return Ok(());
    }
    install(w, color)?;
    w.flush()?;
    Ok(())
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
    let (completed, unchanged) = complete_config(&toml, &path, report)?;
    let changed = completed != toml;
    if changed {
        fs::write(&path, completed)?;
    }
    Ok((unchanged, changed))
}

/// Add to the configuration, which must be valid, the entries
/// of the default one it doesn't have.
///
/// Return the new configuration and the number of entries which were
/// already the default ones.
fn complete_config(
    toml: &str,
    path: &Path,
    report: &mut Vec<String>,
) -> GagaResult<(String, usize)> {
    Config::parse(toml, path.to_path_buf())?;
    // values are compared in tables, and added in documents, which keep
    // the comments and the formatting
    let user = parse_table(toml, path)?;
    let mut doc: DocumentMut = toml.parse()?;
    let default_path = Path::new("default-config.toml");
    let default_config = Config::parse(DEFAULT_CONFIG, default_path.to_path_buf())?;
    let defaults = parse_table(DEFAULT_CONFIG, default_path)?;
    let default_doc: DocumentMut = DEFAULT_CONFIG.parse()?;
    let mut unchanged = 0;
    for entry in DefaultEntry::list(&defaults) {
        match entry.find_in(&user) {
            Some(value) if value == entry.value => unchanged += 1,
            Some(_) => report.push(format!(
                "Not changing your **{}** definition - remove it to reset it to `{}`",
                entry.label(),
                entry.description(&default_config),
            )),
            None => {
                entry.copy(&default_doc, &mut doc);
                report.push(format!("Adding **{}**", entry.label()));
            }
        }
    }
    Ok((doc.to_string(), unchanged))
}

fn parse_table(
    toml: &str,
    path: &Path,
) -> GagaResult<toml::Table> {
    toml::from_str(toml).map_err(|source| GagaError::Config {
        path: path.to_path_buf(),
        source,
    })
}

/// An entry of the default configuration: either a top level value,
/// or a value of a top level table
struct DefaultEntry<'d> {
    section: Option<&'d str>,
    name: &'d str,
    value: &'d toml::Value,
}

impl<'d> DefaultEntry<'d> {
    fn list(defaults: &'d toml::Table) -> Vec<Self> {
        let mut entries = Vec::new();
        for (key, value) in defaults {
            match value {
                toml::Value::Table(table) => {
                    entries.extend(table.iter().map(|(name, value)| Self {
                        section: Some(key),
                        name,
                        value,
                    }));
                }
                _ => entries.push(Self {
                    section: None,
                    name: key,
                    value,
                }),
            }
        }
        entries
    }
    fn find_in<'t>(
        &self,
        table: &'t toml::Table,
    ) -> Option<&'t toml::Value> {
        match self.section {
            Some(section) => table.get(section)?.get(self.name),
            None => table.get(self.name),
        }
    }
    /// How the entry is called in the report
    fn label(&self) -> String {
        match self.section {
            Some("commands") => format!("gg {}", self.name),
            _ => self.name.to_string(),
        }
    }
    /// What the default value is, for the report
    fn description(
        &self,
        default_config: &Config,
    ) -> String {
        match (self.section, self.value.as_str()) {
            (Some("aliases"), Some(target)) => match default_config.alias_command(target) {
                Some(tokens) => tokens.join(" "),
                None => format!("gg {target}"),
            },
            (None, Some(value)) => value.to_string(),
            _ => self.value.to_string(),
        }
    }
    /// Copy the entry from the default document to another one,
    /// which must be a valid configuration without this entry
    fn copy(
        &self,
        default_doc: &DocumentMut,
        doc: &mut DocumentMut,
    ) {
        match self.section {
            Some(section) => {
                let item = default_doc[section][self.name].clone();
                let table = doc.entry(section).or_insert_with(|| {
                    let mut table = Table::new();
                    table.set_implicit(true);
                    Item::Table(table)
                });
                table[self.name] = item;
            }
            None => {
                doc.insert(self.name, default_doc[self.name].clone());
            }
        }
    }
}

#[test]
fn test_is_installed() {
    assert!(is_installed("alias a=b\neval \"$(gaga init zsh)\"\n"));
    assert!(!is_installed("alias a=b\n# eval \"$(gaga init zsh)\"\n"));
    assert!(!is_installed(""));
}

#[cfg(test)]
mod config_tests {
    use super::*;

    fn complete(toml: &str) -> GagaResult<(String, usize, Vec<String>)> {
        let mut report = Vec::new();
        let (completed, unchanged) = complete_config(toml, Path::new("test.toml"), &mut report)?;
        Ok((completed, unchanged, report))
    }

    fn table(toml: &str) -> toml::Table {
        toml::from_str(toml).unwrap()
    }

    #[test]
    fn test_default_config_is_complete() {
        let (completed, unchanged, report) = complete(DEFAULT_CONFIG).unwrap();
        assert_eq!(completed, DEFAULT_CONFIG);
        assert!(report.is_empty());
        assert!(unchanged > 10);
    }

    #[test]
    fn test_empty_config_gets_all_defaults() {
        let (completed, unchanged, report) = complete("").unwrap();
        assert_eq!(table(&completed), table(DEFAULT_CONFIG));
        assert_eq!(unchanged, 0);
        assert!(report.iter().all(|line| line.starts_with("Adding **")));
    }

    #[test]
    fn test_inline_tables_are_completed() {
        let (completed, _, report) = complete("aliases = { gs = \"status\" }\n").unwrap();
        assert!(report.contains(&"Adding **ga**".to_string()));
        let completed = table(&completed);
        assert_eq!(completed["aliases"]["gs"].as_str(), Some("status"));
        assert_eq!(completed["aliases"]["ga"].as_str(), Some("add"));
    }

    #[test]
    fn test_invalid_config_is_refused() {
        for toml in ["bare_command = \"br\"\n", "aliases = 3\n", "[aliases"] {
            assert!(matches!(
                complete(toml),
                Err(GagaError::Config { path, .. }) if path == Path::new("test.toml"),
            ));
        }
    }

    #[test]
    fn test_changed_entries_are_kept() {
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
        let (completed, unchanged, report) = complete(toml).unwrap();
        assert_eq!(unchanged, 2);
        assert!(completed.starts_with("# my config\nmax_listed_on_enter = 20\n"));
        assert!(completed.contains("gs = \"status\" # kept\nga = \"add -p\"\ngl = \"log\"\n"));
        for line in [
            "Adding **gd**",
            "Adding **jump_function**",
            "Adding **gg add**",
            "Not changing your **max_listed_on_enter** definition - remove it to reset it to `50`",
            "Not changing your **ga** definition - remove it to reset it to `git add`",
            "Not changing your **gg rm** definition - remove it to reset it to \
            `{ command = [\"rm\", \"-r\", \"--\"], numbers = true, status_after = true }`",
        ] {
            assert!(report.contains(&line.to_string()), "missing: {line}");
        }
        let config = Config::parse(&completed, "test".into()).unwrap();
        assert_eq!(config.aliases["ga"], "add -p");
        assert_eq!(config.commands["rm"].command, ["trash"]);
        assert_eq!(config.commands["log"].command, ["git", "log"]);
        assert_eq!(config.commands["diff"].command, ["git", "diff"]);

        // installing again changes nothing
        let (again, _, report) = complete(&completed).unwrap();
        assert_eq!(again, completed);
        assert!(report.iter().all(|line| line.starts_with("Not changing")));
    }
}
