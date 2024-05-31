#![forbid(unsafe_code)]
#![windows_subsystem = "windows"]
mod app;
mod counter_or_timer;

use std::sync::OnceLock;
use std::time::Duration;
use tokio::runtime::{Builder, Runtime};

pub const NOTIFICATION_TIMEOUT:u64 = 30;
pub const PERIOD:Duration = Duration::from_secs(1);


static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn get_runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to initialize tokio runtime")
    })
}
#[cfg(debug_assertions)]
const LOG_LEVEL:log::LevelFilter = log::LevelFilter::Debug;
#[cfg(not(debug_assertions))]
const LOG_LEVEL:log::LevelFilter = log::LevelFilter::Info;

fn main() -> Result<(), ()> {
    simple_logger::SimpleLogger::new()
        .with_utc_timestamps()
        .with_colors(true)
        .with_level(LOG_LEVEL)
        .with_module_level("eframe", log::LevelFilter::Info)
        .env()
        .init()
        .expect("Failed to initialize logger");
    log::info!("Logger initialized");
    let rt = get_runtime();
    let _a = rt.enter(); // "_" as a variable name immediately drops the value, causing no tokio runtime to be registered. "_a" does not.
    log::info!("Tokio Runtime initialized");
    let native_options = eframe::NativeOptions::default();
    if let Some(err) = eframe::run_native(
        "Counter",
        native_options,
        Box::new(|cc| Box::new(app::App::new(cc))),
    ).err() {
        log::error!(
            "Error in eframe whilst trying to start the application: {}",
            err
        );
    }
    log::info!("GUI exited. Thank you for using this counter app!");
    Ok(())
}