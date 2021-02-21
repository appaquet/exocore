use log::{Level, Log, Metadata, Record, SetLoggerError};

use crate::binding::__exocore_host_log;

static LOGGER: HostLogger = HostLogger {};

struct HostLogger {}

impl Log for HostLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        log_record(record);
    }

    fn flush(&self) {}
}

fn log_record(record: &Record) {
    unsafe {
        let level: usize = record.level() as usize;
        let s = format!("{}", record.args());
        __exocore_host_log(level as u8, s.as_ptr(), s.len());
    }
}

pub fn init() -> Result<(), SetLoggerError> {
    init_with_level(Level::Info)
}

pub fn init_with_level(level: Level) -> Result<(), SetLoggerError> {
    log::set_logger(&LOGGER)?;
    log::set_max_level(level.to_level_filter());
    Ok(())
}
