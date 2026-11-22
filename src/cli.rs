use {
    crate::*,
    clap::FromArgMatches,
    fs_err as fs,
    std::{
        env,
        ffi::OsString,
        io::{
            self,
            Write,
        },
        os::unix::ffi::OsStrExt,
        path::{
            Path,
            PathBuf,
        },
        process::Command,
    },
};

/// Do what the launch arguments ask for
pub fn run() -> GagaResult<()> {
    init_cli_log!();
    let config = Config::load();
    let matches = Args::command_with(config.as_ref().ok()).get_matches();
    let args = Args::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    let color = args.color();
    let mut out = io::BufWriter::new(io::stdout().lock());
    let Some(action) = args.action else {
        return install_or_help(&mut out, color, config.as_ref().ok());
    };
    match action {
        Action::Install => {
            install(&mut out, color)?;
        }
        Action::Init { shell } => {
            write!(out, "{}", init_script(shell, &config?))?;
        }
        Action::Status => {
            remember_and_print_status(&mut out, color, None, true)?;
        }
        Action::Repos => {
            let db = Db::open()?;
            forget_missing_repos(&db)?;
            for repo in db.repos()? {
                writeln!(out, "{}", repo.name())?;
            }
        }
        Action::Jump { name } => {
            let db = Db::open()?;
            forget_missing_repos(&db)?;
            let repos = db.repos()?;
            let repo = best_match(&repos, &name).ok_or(GagaError::NoRepo(name))?;
            out.write_all(repo.path.as_os_str().as_bytes())?;
            out.write_all(b"\n")?;
        }
        Action::Enter { previous } => {
            if let Err(e) = enter(&mut out, &previous, color, config) {
                warn!("enter failed: {e}");
            }
        }
        Action::Custom(args) => {
            run_custom(&mut out, &args, color, &config?)?;
        }
    }
    out.flush()?;
    Ok(())
}

/// The id of the shell session, given by the shell functions
fn session() -> Option<u32> {
    env::var("GAGA_SESSION").ok()?.parse().ok()
}

/// Print the status if the current directory is in a repository
/// which isn't the one of the previous directory
fn enter<W: Write>(
    w: &mut W,
    previous: &Path,
    color: bool,
    config: GagaResult<Config>,
) -> GagaResult<()> {
    let cwd = env::current_dir().map_err(GagaError::Cwd)?;
    let Some(root) = find_root(&cwd) else {
        return Ok(());
    };
    if let Some(previous_root) = find_root(previous)
        && fs::canonicalize(previous_root).is_ok_and(|previous_root| previous_root == root)
    {
        return Ok(());
    }
    remember_and_print_status(w, color, config?.max_listed_on_enter, false)
}

/// Remove from the global memory the repositories which don't exist anymore
fn forget_missing_repos(db: &Db) -> GagaResult<()> {
    for repo in db.repos()?.iter().filter(|repo| !repo.exists()) {
        db.forget_repo(repo)?;
    }
    Ok(())
}

/// Remember the current repository and its changed files, then print
/// its status.
///
/// A failure to write in the global memory is an error only when `strict`.
fn remember_and_print_status<W: Write>(
    w: &mut W,
    color: bool,
    max_listed: Option<usize>,
    strict: bool,
) -> GagaResult<()> {
    let cwd = env::current_dir().map_err(GagaError::Cwd)?;
    let root = find_root(&cwd).ok_or(GagaError::NotInRepo)?;
    let status = time!(Status::read(&cwd))?;
    let unnumbered = match max_listed {
        Some(max) if status.files.len() > max => FileListing::Count,
        _ => FileListing::List,
    };
    let listing = match time!(remember_status(root, &status, unnumbered)) {
        Ok(listing) => listing,
        Err(e) if strict => return Err(e),
        Err(e) => {
            warn!("global memory not updated: {e}");
            unnumbered
        }
    };
    let printer = StatusPrinter {
        color,
        listing,
        cwd: &cwd,
        root,
    };
    printer.write(w, &status)?;
    Ok(())
}

/// Record in the global memory the visit of the repository and, when
/// they're listed in a shell session, its changed files as the numbered
/// ones of this session.
///
/// Return the listing, changed to a numbered one when the files are recorded.
fn remember_status(
    root: &Path,
    status: &Status,
    listing: FileListing,
) -> GagaResult<FileListing> {
    let mut db = Db::open()?;
    db.visit(root)?;
    let (FileListing::List, Some(session)) = (listing, session()) else {
        return Ok(listing);
    };
    let paths: Vec<PathBuf> = status
        .files
        .iter()
        .map(|file| root.join(&file.path))
        .collect();
    db.set_files(session, &paths)?;
    Ok(FileListing::NumberedList)
}

/// Run a command of the configuration, `args` being its name
/// followed by its arguments
fn run_custom<W: Write>(
    w: &mut W,
    args: &[OsString],
    color: bool,
    config: &Config,
) -> GagaResult<()> {
    let Some((name, args)) = args.split_first() else {
        return Ok(());
    };
    let name = name.to_string_lossy();
    let command = config
        .commands
        .get(name.as_ref())
        .ok_or_else(|| GagaError::UnknownCommand(name.to_string()))?;
    let mut tokens = command.tokens()?;
    if command.numbers {
        let load_files = || {
            let session = session().ok_or(GagaError::NoSession)?;
            Db::open()?.files(session)
        };
        tokens.extend(resolve_args(args, load_files)?);
    } else {
        tokens.extend_from_slice(args);
    }
    debug!("running {tokens:?}");
    let Some((exe, exe_args)) = tokens.split_first().filter(|(exe, _)| !exe.is_empty()) else {
        return Err(GagaError::EmptyCommand(name.to_string()));
    };
    let exit_status = Command::new(exe)
        .args(exe_args)
        .status()
        .map_err(|source| GagaError::Spawn {
            exe: exe.to_string_lossy().into_owned(),
            source,
        })?;
    if !exit_status.success() {
        let code = exit_status.code().and_then(|code| u8::try_from(code).ok());
        return Err(GagaError::CommandFailed(code.unwrap_or(1)));
    }
    if command.status_after {
        remember_and_print_status(w, color, None, true)?;
    }
    Ok(())
}
