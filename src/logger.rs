use colored::Colorize;
use log::{SetLoggerError, LevelFilter, Record, Level, Metadata};

static LOGGER: SimpleLogger = SimpleLogger;

pub fn init() -> Result<(), SetLoggerError> {
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(LevelFilter::Info))
}

struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
	        match record.level() {
	            Level::Error => println!("{}", format!("{}", record.args()).red()),
	            Level::Warn => println!("{}", format!("{}", record.args()).blue()),
	            Level::Info => println!("{}", record.args()),
	            Level::Debug => println!("{}", format!("{}", record.args()).purple()),
	            Level::Trace => println!("{}", record.args()),
	        }
        }
    }

    fn flush(&self) {}
}
