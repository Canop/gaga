use {
    crate::*,
    clap::FromArgMatches,
    std::{
        env,
        fs,
        io::{
            self,
            Write,
        },
        path::Path,
        process::Command,
    },
};

pub fn run() -> GagaResult<()> {
    init_cli_log!();
    let config = Config::load();
    let matches = Args::command_with(config.as_ref().ok()).get_matches();
    let args = Args::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    let color = args.color();
    let Some(action) = args.action else {
        return install_or_help(config.as_ref().ok());
    };
    match action {
        Action::Install => {
            install()?;
        }
        Action::Init { shell } => {
            print!("{}", init_script(shell, &config?));
        }
        Action::Status => {
            print_status(color, None, true)?;
        }
        Action::Repos => {
            for repo in known_repos()? {
                println!("{}", repo.name());
            }
        }
        Action::Jump { name } => {
            let repos = known_repos()?;
            let repo = best_match(&repos, &name).ok_or(GagaError::NoRepo(name))?;
            println!("{}", repo.path);
        }
        Action::Enter { previous } => {
            if let Err(e) = enter(&previous, color, config) {
                warn!("enter failed: {e}");
            }
        }
        Action::Custom(args) => {
            run_custom(&args, color, &config?)?;
        }
    }
    Ok(())
}

/// The id of the shell session, given by the shell functions
fn session() -> Option<u32> {
    env::var("GAGA_SESSION").ok()?.parse().ok()
}

fn enter(
    previous: &Path,
    color: bool,
    config: GagaResult<Config>,
) -> GagaResult<()> {
    let cwd = env::current_dir()?;
    let Some(root) = find_root(&cwd) else {
        return Ok(());
    };
    if let Some(previous_root) = find_root(previous)
        && fs::canonicalize(previous_root).is_ok_and(|previous_root| previous_root == root)
    {
        return Ok(());
    }
    print_status(color, config?.max_listed_on_enter, false)
}

/// The repositories of the global memory, last visited first,
/// the ones which don't exist anymore being forgotten
fn known_repos() -> GagaResult<Vec<Repo>> {
    let db = Db::open()?;
    let mut repos = db.repos()?;
    for repo in repos.iter().filter(|repo| !repo.exists()) {
        db.forget_repo(repo)?;
    }
    repos.retain(Repo::exists);
    Ok(repos)
}

/// Print the status of the current repository, which is remembered
/// as visited.
///
/// Files are numbered when their list can be saved for the session.
/// A failure to write in the global memory is an error only when `strict`.
fn print_status(
    color: bool,
    max_listed: Option<usize>,
    strict: bool,
) -> GagaResult<()> {
    let cwd = env::current_dir()?;
    let root = find_root(&cwd).ok_or(GagaError::NotInRepo)?;
    let status = time!(Status::read(&cwd))?;
    let listed = max_listed.is_none_or(|max| status.files.len() <= max);
    let save = || -> GagaResult<bool> {
        let mut db = Db::open()?;
        db.visit(root)?;
        let Some(session) = session().filter(|_| listed) else {
            return Ok(false);
        };
        let paths: Vec<String> = status
            .files
            .iter()
            .map(|file| root.join(&file.path).to_string_lossy().into_owned())
            .collect();
        db.set_files(session, &paths)?;
        Ok(true)
    };
    let numbered = match time!(save()) {
        Ok(numbered) => numbered,
        Err(e) if strict => return Err(e),
        Err(e) => {
            warn!("global memory not updated: {e}");
            false
        }
    };
    let printer = StatusPrinter {
        color,
        listed,
        numbered,
        cwd: &cwd,
        root,
    };
    let mut w = io::BufWriter::new(io::stdout().lock());
    printer.write(&mut w, &status)?;
    w.flush()?;
    Ok(())
}

/// Run a command of the configuration, `args` being its name
/// followed by its arguments
fn run_custom(
    args: &[String],
    color: bool,
    config: &Config,
) -> GagaResult<()> {
    let Some((name, args)) = args.split_first() else {
        return Ok(());
    };
    let command = config
        .commands
        .get(name)
        .ok_or_else(|| GagaError::UnknownCommand(name.clone()))?;
    let mut tokens = command.tokens();
    if command.numbers && args.iter().any(|arg| number_range(arg).is_some()) {
        let session = session().ok_or(GagaError::NoSession)?;
        let files = Db::open()?.files(session)?;
        tokens.extend(resolve_args(args, &files)?);
    } else {
        tokens.extend_from_slice(args);
    }
    debug!("running {tokens:?}");
    let Some((exe, exe_args)) = tokens.split_first().filter(|(exe, _)| !exe.is_empty()) else {
        return Err(GagaError::EmptyCommand(name.clone()));
    };
    let exit_status = Command::new(exe)
        .args(exe_args)
        .status()
        .map_err(|source| GagaError::Spawn {
            exe: exe.clone(),
            source,
        })?;
    if !exit_status.success() {
        let code = exit_status.code().and_then(|code| u8::try_from(code).ok());
        return Err(GagaError::CommandFailed(code.unwrap_or(1)));
    }
    if command.status_after {
        print_status(color, None, true)?;
    }
    Ok(())
}
