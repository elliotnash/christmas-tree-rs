use serde::{Deserialize, Serialize};
use log::Level;

/// RGB color value
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// Payload for SetLeds message
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetLedsPayload {
    pub leds: Vec<Rgb>,
}

/// Serializable wrapper for log::Level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SerializableLogLevel(Level);

impl SerializableLogLevel {
    /// Create a new SerializableLogLevel from a log::Level
    pub fn new(level: Level) -> Self {
        Self(level)
    }

    /// Get the inner log::Level
    pub fn level(&self) -> Level {
        self.0
    }
}

impl From<Level> for SerializableLogLevel {
    fn from(level: Level) -> Self {
        Self(level)
    }
}

impl From<SerializableLogLevel> for Level {
    fn from(level: SerializableLogLevel) -> Self {
        level.0
    }
}

impl Serialize for SerializableLogLevel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self.0 {
            Level::Error => serializer.serialize_str("error"),
            Level::Warn => serializer.serialize_str("warn"),
            Level::Info => serializer.serialize_str("info"),
            Level::Debug => serializer.serialize_str("debug"),
            Level::Trace => serializer.serialize_str("trace"),
        }
    }
}

impl<'de> Deserialize<'de> for SerializableLogLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let level = match s.as_str() {
            "error" => Level::Error,
            "warn" => Level::Warn,
            "info" => Level::Info,
            "debug" => Level::Debug,
            "trace" => Level::Trace,
            _ => return Err(serde::de::Error::custom(format!("Invalid log level: {}", s))),
        };
        Ok(Self(level))
    }
}

/// Payload for Log message
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogPayload {
    pub level: SerializableLogLevel,
    pub content: String,
}

impl LogPayload {
    /// Create a new LogPayload from a log::Level and content
    pub fn new(level: Level, content: String) -> Self {
        Self {
            level: SerializableLogLevel::from(level),
            content,
        }
    }

    /// Get the log level
    pub fn level(&self) -> Level {
        self.level.level()
    }
}

/// Message type enum
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Message {
    /// Heartbeat message with no payload
    Heartbeat,
    /// Set LED values message with RGB array payload
    SetLeds(SetLedsPayload),
    /// Log message with log level and string content
    Log(LogPayload),
}

impl Message {
    /// Serialize message to bytes using postcard
    pub fn to_bytes(&self) -> Result<Vec<u8>, postcard::Error> {
        postcard::to_allocvec(self)
    }

    /// Deserialize message from bytes using postcard
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, postcard::Error> {
        postcard::from_bytes(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_serialization() {
        let msg = Message::Heartbeat;
        let bytes = msg.to_bytes().unwrap();
        let deserialized = Message::from_bytes(&bytes).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn set_leds_serialization() {
        let payload = SetLedsPayload {
            leds: vec![
                Rgb::new(255, 0, 0),
                Rgb::new(0, 255, 0),
                Rgb::new(0, 0, 255),
            ],
        };
        let msg = Message::SetLeds(payload);
        let bytes = msg.to_bytes().unwrap();
        let deserialized = Message::from_bytes(&bytes).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn log_serialization() {
        let payload = LogPayload::new(Level::Info, "System initialized".to_string());
        let msg = Message::Log(payload);
        let bytes = msg.to_bytes().unwrap();
        let deserialized = Message::from_bytes(&bytes).unwrap();
        assert_eq!(msg, deserialized);
    }
}
