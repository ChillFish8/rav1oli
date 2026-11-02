use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, bail};

/// The exit status of a process stopped by Ctrl-C.
const INTERRUPTED_STATUS: i32 = 130;

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// Installs the Ctrl-C handler.
///
/// The first Ctrl-C asks the running work to stop so it can clean up. A second one exits at once.
pub fn install() -> anyhow::Result<()> {
    ctrlc::set_handler(on_interrupt).context("could not install the Ctrl-C handler")
}

fn on_interrupt() {
    let was_interrupted = INTERRUPTED.swap(true, Ordering::SeqCst);
    if was_interrupted {
        std::process::exit(INTERRUPTED_STATUS);
    }

    tracing::warn!("interrupted, stopping and cleaning up. Press Ctrl-C again to exit immediately");
}

/// Errors once Ctrl-C has been pressed.
pub fn check() -> anyhow::Result<()> {
    if INTERRUPTED.load(Ordering::SeqCst) {
        bail!("interrupted by Ctrl-C");
    }

    Ok(())
}

#[cfg(test)]
pub fn set_interrupted(is_interrupted: bool) {
    INTERRUPTED.store(is_interrupted, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_errors_once_interrupted() {
        check().expect("nothing has interrupted the run yet");

        set_interrupted(true);
        let error = check().expect_err("the run was interrupted");
        set_interrupted(false);

        assert_eq!(error.to_string(), "interrupted by Ctrl-C");
        check().expect("the flag was reset");
    }
}
