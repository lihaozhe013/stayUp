use std::path::Path;

pub fn initialize(directory: &Path) {
    if std::fs::create_dir_all(directory).is_err() {
        return;
    }
    let Ok(appender) = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("stayup")
        .filename_suffix("log")
        .max_log_files(7)
        .build(directory)
    else {
        return;
    };
    let (writer, _guard) = tracing_appender::non_blocking(appender);
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_ansi(false)
        .with_writer(writer)
        .try_init();
    std::mem::forget(_guard);
}
