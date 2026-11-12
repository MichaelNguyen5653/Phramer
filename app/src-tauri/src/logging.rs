//! Log lines to stderr, `phramer: <level>: <message>` as 15.0.0's
//! `AbstractLogger` wrote them. A release build has no console, so they are
//! seen only when stderr is redirected.

use log::{Level, LevelFilter, Log, Metadata, Record};

struct StderrLogger;

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let level = match record.level() {
            Level::Error => "error",
            Level::Warn => "warning",
            _ => "info",
        };
        // Only this crate's own lines; dependencies log through `log` too
        if record.target().starts_with(env!("CARGO_CRATE_NAME")) {
            eprintln!("phramer: {level}: {}", record.args());
        }
    }

    fn flush(&self) {}
}

static LOGGER: StderrLogger = StderrLogger;

pub(crate) fn init() {
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(LevelFilter::Info);
    }
}
