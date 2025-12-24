use common::message::Message;
use postcard::to_allocvec;
use serialport::SerialPort;
use std::io::{Read, Write};
use std::sync::Mutex;
use std::time::Duration;

/// Frame delimiter byte (0x00) - COBS ensures this never appears in encoded data
const FRAME_DELIMITER: u8 = 0x00;

/// Serial message handler for sending and receiving messages over serial port using COBS framing
pub struct MessageHandler {
    port: Mutex<Box<dyn SerialPort>>,
    receive_buffer: Mutex<Vec<u8>>,
    last_read_time: Mutex<Option<std::time::Instant>>,
}

impl MessageHandler {
    /// Create a new MessageHandler by opening a serial port
    pub fn new(port_path: &str, baud_rate: u32) -> Result<Self, MessageError> {
        let port = serialport::new(port_path, baud_rate)
            .timeout(Duration::from_millis(10))
            .open()
            .map_err(|e| MessageError::PortError(format!("Failed to open serial port: {}", e)))?;

        Ok(Self {
            port: Mutex::new(port),
            receive_buffer: Mutex::new(Vec::new()),
            last_read_time: Mutex::new(None),
        })
    }

    /// Send a message over serial using COBS encoding with frame delimiter
    pub fn send(&self, message: &Message) -> Result<(), MessageError> {
        // Serialize message to bytes using postcard
        let serialized = to_allocvec(message)
            .map_err(|e| MessageError::Serialization(format!("Postcard serialization error: {}", e)))?;

        // Encode with COBS
        let mut encoded = cobs::encode_vec(&serialized)
            .map_err(|e| MessageError::Serialization(format!("COBS encoding error: {}", e)))?;

        // Append frame delimiter (byte 0)
        encoded.push(FRAME_DELIMITER);

        // Write to serial port - handle partial writes
        if let Ok(mut port) = self.port.lock() {
            let mut remaining = &encoded[..];
            while !remaining.is_empty() {
                match port.write(remaining) {
                    Ok(0) => return Err(MessageError::WriteError("No progress on write".to_string())),
                    Ok(n) => remaining = &remaining[n..],
                    Err(e) => return Err(MessageError::WriteError(format!("Serial write error: {}", e))),
                }
            }
            port.flush()
                .map_err(|e| MessageError::WriteError(format!("Serial flush error: {}", e)))?;
            Ok(())
        } else {
            Err(MessageError::LockError)
        }
    }

    /// Try to receive a message from serial
    /// Returns Ok(Some(message)) if a complete frame was received (ending with byte 0)
    /// Returns Ok(None) if no complete frame is available yet
    /// Returns Err if an error occurred
    pub fn try_receive(&self) -> Result<Option<Message>, MessageError> {
        let mut new_bytes_received = false;

        // Read from serial port
        let bytes_read = {
            if let Ok(mut port) = self.port.lock() {
                let mut buffer = [0u8; 256];

                // Read available bytes from serial (non-blocking due to timeout)
                match port.read(&mut buffer) {
                    Ok(n) if n > 0 => {
                        new_bytes_received = true;
                        Some(buffer[..n].to_vec())
                    }
                    Ok(_) => None, // No data available
                    Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => None, // No data available
                    Err(e) => return Err(MessageError::ReadError(format!("Serial read error: {}", e))),
                }
            } else {
                return Err(MessageError::LockError);
            }
        };

        // Append new data to receive buffer if any was read
        if let Some(new_data) = bytes_read {
            if let Ok(mut recv_buf) = self.receive_buffer.lock() {
                recv_buf.extend_from_slice(&new_data);
            } else {
                return Err(MessageError::LockError);
            }
        }

        // Update last read time if we received new bytes
        if new_bytes_received {
            if let Ok(mut last_read) = self.last_read_time.lock() {
                *last_read = Some(std::time::Instant::now());
            }
        }

        // Look for complete frame (ending with byte 0)
        if let Ok(mut recv_buf) = self.receive_buffer.lock() {
            if recv_buf.is_empty() {
                return Ok(None);
            }

            // Find frame delimiter (byte 0)
            if let Some(frame_end) = recv_buf.iter().position(|&b| b == FRAME_DELIMITER) {
                // Found a complete frame
                let frame_data = recv_buf[..frame_end].to_vec();

                // Remove the frame (including delimiter) from buffer
                recv_buf.drain(..=frame_end);

                // Decode COBS
                let decoded: Result<Vec<u8>, _> = cobs::decode_vec(&frame_data);
                let decoded_bytes = decoded
                    .map_err(|e| MessageError::DecodeError(format!("COBS decode error: {}", e)))?;

                // Deserialize message
                let message = Message::from_bytes(&decoded_bytes)
                    .map_err(|e| MessageError::Deserialization(format!("Postcard deserialization error: {}", e)))?;

                return Ok(Some(message));
            } else {
                // No complete frame yet
                // If buffer is getting too large, clear it to prevent memory issues
                if recv_buf.len() > 4096 {
                    recv_buf.clear();
                    return Err(MessageError::BufferOverflow);
                }
            }
        }

        Ok(None)
    }

    /// Blocking receive that waits for a message
    /// This will block until a complete message is received or timeout occurs
    pub fn receive(&self, timeout: Duration) -> Result<Message, MessageError> {
        let start = std::time::Instant::now();

        loop {
            match self.try_receive() {
                Ok(Some(message)) => return Ok(message),
                Ok(None) => {
                    if start.elapsed() >= timeout {
                        return Err(MessageError::Timeout);
                    }
                    // Small delay to avoid busy waiting
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// Errors that can occur when handling messages
#[derive(Debug)]
pub enum MessageError {
    Serialization(String),
    Deserialization(String),
    DecodeError(String),
    WriteError(String),
    ReadError(String),
    PortError(String),
    LockError,
    Timeout,
    BufferOverflow,
}

impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageError::Serialization(e) => write!(f, "Serialization error: {}", e),
            MessageError::Deserialization(e) => write!(f, "Deserialization error: {}", e),
            MessageError::DecodeError(e) => write!(f, "COBS decode error: {}", e),
            MessageError::WriteError(e) => write!(f, "Write error: {}", e),
            MessageError::ReadError(e) => write!(f, "Read error: {}", e),
            MessageError::PortError(e) => write!(f, "Port error: {}", e),
            MessageError::LockError => write!(f, "Failed to acquire lock"),
            MessageError::Timeout => write!(f, "Receive timeout"),
            MessageError::BufferOverflow => write!(f, "Receive buffer overflow"),
        }
    }
}

impl std::error::Error for MessageError {}
