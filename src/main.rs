use std::process::ExitCode;

fn main() -> ExitCode {
    match gaga::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => e.report(),
    }
}
