use crate::messages::MessageHandler;
use common::message::{LogPayload, Message};
use log::{LevelFilter, Log, Metadata, Record, SetLoggerError};
use std::sync::Arc;

/// Logger that sends log messages over serial UART using MessageHandler
pub struct SerialLogger {
    message_handler: Arc<MessageHandler>,
}

impl SerialLogger {
    /// Create a new SerialLogger with a MessageHandler
    pub fn new(message_handler: Arc<MessageHandler>) -> Self {
        Self { message_handler }
    }

    /// Initialize the logger as the global logger
    pub fn init(self, max_level: LevelFilter) -> Result<(), SetLoggerError> {
        log::set_boxed_logger(Box::new(self))?;
        log::set_max_level(max_level);
        Ok(())
    }
}

impl Log for SerialLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        // Let the log crate's max_level filter handle this
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            // Create log payload from the record
            let content = format!("{}", record.args());
            let payload = LogPayload::new(record.level(), content);
            let message = Message::Log(payload);

            // Send message through MessageHandler
            // Ignore errors in logging to avoid infinite loops
            let _ = self.message_handler.send(&message);
        }
    }

    fn flush(&self) {
        // MessageHandler handles flushing internally
        // No additional action needed here
    }
}
