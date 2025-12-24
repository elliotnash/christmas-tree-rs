use common::message::{LogPayload, Message};
use esp_idf_svc::hal::uart::UartDriver;
use log::{LevelFilter, Log, Metadata, Record, SetLoggerError};
use std::sync::Mutex;

/// Logger that sends log messages over serial UART
pub struct SerialLogger {
    uart: Mutex<UartDriver<'static>>,
}

impl SerialLogger {
    /// Create a new SerialLogger with a UART driver
    pub fn new(uart: UartDriver<'static>) -> Self {
        Self {
            uart: Mutex::new(uart),
        }
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

            // Serialize the message
            if let Ok(bytes) = message.to_bytes() {
                // Write to UART - UartDriver implements esp_idf_svc::io::Write
                if let Ok(mut uart) = self.uart.lock() {
                    use esp_idf_svc::io::Write;
                    // Write all bytes, handling partial writes
                    let mut remaining = &bytes[..];
                    while !remaining.is_empty() {
                        match uart.write(remaining) {
                            Ok(0) => break, // No progress, stop trying
                            Ok(n) => remaining = &remaining[n..],
                            Err(_) => break, // Error occurred, stop trying
                        }
                    }
                }
            }
        }
    }

    fn flush(&self) {
        // UART writes are typically immediate, but we can ensure flush if needed
        if let Ok(mut uart) = self.uart.lock() {
            use esp_idf_svc::io::Write;
            let _ = uart.flush();
        }
    }
}
