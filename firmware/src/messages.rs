use common::message::Message;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time;
use esp_idf_svc::hal::uart::{UartTxDriver, UartRxDriver};
use esp_idf_svc::io::asynch::Write;

/// Frame delimiter byte (0x00) - COBS ensures this never appears in encoded data
const FRAME_DELIMITER: u8 = 0x00;

/// Channel sizes for messages
const RX_CHANNEL_SIZE: usize = 16;
const TX_CHANNEL_SIZE: usize = 16;

/// Static channels for messages
pub static RX_CHANNEL: Channel<CriticalSectionRawMutex, Message, RX_CHANNEL_SIZE> = Channel::new();
pub static TX_CHANNEL: Channel<CriticalSectionRawMutex, Message, TX_CHANNEL_SIZE> = Channel::new();

/// UART TX task that continuously reads messages from TX_CHANNEL and sends them over UART1
#[embassy_executor::task]
pub async fn tx_task(
    mut uart_tx: esp_idf_svc::hal::uart::AsyncUartTxDriver<'static, UartTxDriver<'static>>,
) {
    let receiver = TX_CHANNEL.receiver();

    loop {
        // Wait for a message to send
        let message = receiver.receive().await;

        // Serialize and COBS encode message (includes 0x00 delimiter at the end)
        let encoded = match postcard::to_stdvec_cobs(&message) {
            Ok(data) => data,
            Err(_e) => {
                // Skip invalid messages
                continue;
            }
        };

        // Write to UART - handle partial writes
        let mut remaining = &encoded[..];
        while !remaining.is_empty() {
            match uart_tx.write(remaining).await {
                Ok(0) => {
                    // No progress on write, yield and retry
                    embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
                    continue;
                }
                Ok(n) => remaining = &remaining[n..],
                Err(_e) => {
                    // Write error, skip this message
                    break;
                }
            }
        }

        // Flush to ensure data is sent
        let _ = uart_tx.flush().await;
    }
}

/// UART RX task that continuously reads from UART and pushes complete messages to RX_CHANNEL
#[embassy_executor::task]
pub async fn rx_task(
    uart_rx: esp_idf_svc::hal::uart::AsyncUartRxDriver<'static, UartRxDriver<'static>>,
) {
    let mut receive_buffer = Vec::new();

    loop {
        // Read from UART
        let mut buffer = [0u8; 256];
        match uart_rx.read(&mut buffer).await {
            Ok(n) if n > 0 => {
                // Append new data to receive buffer
                receive_buffer.extend_from_slice(&buffer[..n]);
            }
            Ok(_) => {
                // No data available, yield CPU time
                embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
                continue;
            }
            Err(_) => {
                // Error reading, continue
                embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
                continue;
            }
        }

        // Look for complete frames (ending with byte 0)
        loop {
            if receive_buffer.is_empty() {
                break;
            }

            // Find frame delimiter (byte 0)
            if let Some(frame_end) = receive_buffer.iter().position(|&b| b == FRAME_DELIMITER) {
                // Found a potential complete frame (including delimiter at frame_end)
                // Extract frame data (need mutable for from_bytes_cobs)
                let mut frame_data = receive_buffer[..=frame_end].to_vec();

                // Try to decode COBS and deserialize message
                match postcard::from_bytes_cobs::<Message>(&mut frame_data) {
                    Ok(message) => {
                        // Success! Remove the frame (including delimiter) from buffer
                        receive_buffer.drain(..=frame_end);

                        // Send message to RX channel (blocking until space is available)
                        // This ensures messages are not lost
                        let sender = RX_CHANNEL.sender();
                        sender.send(message).await;
                    }
                    Err(_) => {
                        // Deserialization failed - this might be corrupted data
                        // Discard the first byte and continue searching for another delimiter
                        if receive_buffer.len() > 1 {
                            receive_buffer.remove(0);
                            // Continue loop to look for another delimiter
                        } else {
                            receive_buffer.clear();
                            break;
                        }
                    }
                }
            } else {
                // No delimiter found - frame is incomplete
                // If buffer is getting too large, clear it to prevent memory issues
                if receive_buffer.len() > 4096 {
                    receive_buffer.clear();
                }
                break;
            }
        }
    }
}
