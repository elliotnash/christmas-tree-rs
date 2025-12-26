// use common::message::{LogPayload, Message};
// use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
// use embassy_sync::channel::Channel;
// use log::{LevelFilter, Log, Metadata, Record, SetLoggerError};

// /// Logger that sends log messages over serial UART using channels
// pub struct SerialLogger;

// impl SerialLogger {
//     /// Create a new SerialLogger
//     pub fn new() -> Self {
//         Self
//     }

//     /// Initialize the logger as the global logger
//     pub fn init(self, max_level: LevelFilter) -> Result<(), SetLoggerError> {
//         log::set_boxed_logger(Box::new(self))?;
//         log::set_max_level(max_level);
//         Ok(())
//     }

//     /// Get a reference to the log channel for processing messages
//     /// This should be called from the main async loop to process log messages
//     pub fn channel() -> &'static Channel<CriticalSectionRawMutex, Message, 32> {
//         &LOG_CHANNEL
//     }
// }

// impl Log for SerialLogger {
//     fn enabled(&self, _metadata: &Metadata) -> bool {
//         // Let the log crate's max_level filter handle this
//         true
//     }

//     fn log(&self, record: &Record) {
//         if self.enabled(record.metadata()) {
//             // Create log payload from the record
//             let content = format!("{}", record.args());
//             let payload = LogPayload::new(record.level(), content);
//             let message = Message::Log(payload);

//             // Try to send to channel (non-blocking, will drop if channel is full)
//             // This prevents blocking the logger and avoids infinite loops
//             let sender = LOG_CHANNEL.sender();
//             let _ = sender.try_send(message);
//         }
//     }

//     fn flush(&self) {
//         // Channel-based logging handles this automatically
//         // Messages are processed asynchronously in the main loop
//     }
// }
