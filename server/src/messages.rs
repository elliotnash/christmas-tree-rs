use common::message::Message;
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
        // Serialize and COBS encode message (includes 0x00 delimiter at the end)
        let encoded = postcard::to_stdvec_cobs(message)
            .map_err(|e| MessageError::Serialization(format!("Postcard COBS serialization error: {}", e)))?;

        println!("Sending message: {:?}", encoded);

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
    /// 
    /// If bytes are received, continues reading until a complete frame is found or no more data is available
    pub fn try_receive(&self) -> Result<Option<Message>, MessageError> {
        let mut any_bytes_received = false;

        // Keep reading until we have a complete frame or no more data is available
        loop {
            // Read from serial port
            let bytes_read = {
                if let Ok(mut port) = self.port.lock() {
                    let mut buffer = [0u8; 256];

                    // Read available bytes from serial (non-blocking due to timeout)
                    match port.read(&mut buffer) {
                        Ok(n) if n > 0 => {
                            any_bytes_received = true;
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
            } else {
                // No more data available, break out of read loop
                break;
            }
        }

        // Update last read time if we received any bytes
        if any_bytes_received {
            if let Ok(mut last_read) = self.last_read_time.lock() {
                *last_read = Some(std::time::Instant::now());
            }
        }

        // Look for complete frame (ending with byte 0)
        if let Ok(mut recv_buf) = self.receive_buffer.lock() {
            if recv_buf.is_empty() {
                return Ok(None);
            }

            // Keep trying to find and decode valid frames
            loop {
                // Find frame delimiter (byte 0)
                if let Some(frame_end) = recv_buf.iter().position(|&b| b == FRAME_DELIMITER) {
                    // Found a potential complete frame (including delimiter at frame_end)
                    // Extract frame data (need mutable for from_bytes_cobs)
                    let mut frame_data = recv_buf[..=frame_end].to_vec();

                    // Try to decode COBS and deserialize message
                    match postcard::from_bytes_cobs::<Message>(&mut frame_data) {
                        Ok(message) => {
                            // Success! Remove the frame (including delimiter) from buffer
                            recv_buf.drain(..=frame_end);
                            return Ok(Some(message));
                        }
                        Err(_) => {
                            // Deserialization failed - this might be corrupted data
                            // Discard the first byte and continue searching for another delimiter
                            if recv_buf.len() > 1 {
                                recv_buf.remove(0);
                                // Continue loop to look for another delimiter
                            } else {
                                recv_buf.clear();
                                break;
                            }
                        }
                    }
                } else {
                    // No delimiter found - frame is incomplete or buffer is empty
                    // If buffer is getting too large, clear it to prevent memory issues
                    if recv_buf.len() > 4096 {
                        recv_buf.clear();
                        return Err(MessageError::BufferOverflow);
                    }
                    break;
                }
            }
        }

        // Only return None if we didn't receive any new bytes
        // If we received bytes but no delimiter, the frame is incomplete
        if any_bytes_received {
            // We received bytes but no complete frame - wait for more data
            Ok(None)
        } else {
            // No bytes received at all
            Ok(None)
        }
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
