use clap::Parser;

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// Set the log level of the system.
    ///
    /// This is overridden by `RUST_LOG` if set. The same filter pattern
    /// as `RUST_LOG` is supported.
    #[arg(long, global = true, env = "IDOL_LOG_LEVEL")]
    log_level: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Safety: Called from main thread
    unsafe { setup_logging(args.log_level.as_deref()) }

    tracing::info!("initialising");


    Ok(())
}

/// Initialises the logging for the system.
///
/// # Safety
/// Has the same safety requirements of [std::env::set_var].
unsafe fn setup_logging(args_log_level: Option<&str>) {
    if std::env::var("RUST_LOG").is_err() {
        let level = args_log_level.unwrap_or("info");

        unsafe { std::env::set_var("RUST_LOG", level) };
    }

    tracing_subscriber::fmt::init();
}