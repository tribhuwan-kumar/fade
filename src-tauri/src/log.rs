use std::fs;
use anyhow::Result;
use tracing::error;
use std::fs::OpenOptions;
use tauri::{App, Manager};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{prelude::*, EnvFilter, fmt};

pub fn init_logging(app: &App) -> Result<WorkerGuard> {
    let resolver = app.path();
    let app_data_local = resolver
        .app_local_data_dir()?;

    if !app_data_local.exists() {
        fs::create_dir_all(&app_data_local)?;
    }

    let log_path = app_data_local.join("fade.log");

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .write(true)
        .open(&log_path)?;

    let (file_writer, guard) = tracing_appender::non_blocking(file);

    let file_layer = fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false)
        .with_filter(env_filter.clone());

    let registry = tracing_subscriber::registry().with(file_layer);

    let con_registry = {
        let console_layer = fmt::layer()
            .with_writer(std::io::stdout)
            .with_ansi(true)
            .with_filter(env_filter.clone());
        registry.with(console_layer)
    };

    con_registry.init();

    std::panic::set_hook(Box::new(|panic_info| {
        error!("panic occurred: {:?}", panic_info);
    }));

    Ok(guard)
}
