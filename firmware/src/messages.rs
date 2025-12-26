use common::message::Message;
use embassy_futures::yield_now;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use esp_hal::uart::{UartRx, UartTx};
use esp_hal::Async;
use alloc::vec::Vec;

/// Frame delimiter byte (0x00) - COBS ensures this never appears in encoded data
pub const PACKET_DELIMITER: u8 = 0x00;
pub const FIFO_FULL_THRESHOLD: usize = 120;

/// Channel sizes for messages
const RX_CHANNEL_SIZE: usize = 16;
const TX_CHANNEL_SIZE: usize = 16;

/// Static channels for messages
pub static RX_CHANNEL: Channel<CriticalSectionRawMutex, Message, RX_CHANNEL_SIZE> = Channel::new();
pub static TX_CHANNEL: Channel<CriticalSectionRawMutex, Message, TX_CHANNEL_SIZE> = Channel::new();

/// UART TX task that continuously reads messages from TX_CHANNEL and sends them over UART1
#[embassy_executor::task]
pub async fn tx_task(mut uart_tx: UartTx<'static, Async>) {
    let receiver = TX_CHANNEL.receiver();

    loop {
        // Wait for a message to send
        let message = receiver.receive().await;

        // Serialize and COBS encode message (includes 0x00 delimiter at the end)
        let encoded = match postcard::to_allocvec_cobs(&message) {
            Ok(data) => data,
            Err(e) => {
                log::error!("Failed to serialize message: {:?}", e);
                continue;
            }
        };

        // Write to UART - handle partial writes
        let mut remaining = &encoded[..];
        while !remaining.is_empty() {
            match uart_tx.write_async(remaining).await {
                Ok(0) => {
                    // No progress on write, yield and retry
                    yield_now().await;
                }
                Ok(n) => remaining = &remaining[n..],
                Err(e) => {
                    // Write error, skip this message
                    log::error!("Failed to write to UART: {:?}", e);
                    break;
                }
            }
        }

        // Flush to ensure data is sent
        uart_tx.flush_async().await.ok();
    }
}

/// UART RX task that continuously reads from UART and pushes complete messages to RX_CHANNEL
#[embassy_executor::task]
pub async fn rx_task(mut uart_rx: UartRx<'static, Async>) {
    const MAX_BUFFER_SIZE: usize = 10 * FIFO_FULL_THRESHOLD + 16;

    let sender = RX_CHANNEL.sender();

    let mut receive_buffer = Vec::with_capacity(MAX_BUFFER_SIZE);
    let mut read_buffer = [0u8; MAX_BUFFER_SIZE];

    // Continuously read from UART until a packet delimiter is found
    loop {
        match uart_rx.read_async(&mut read_buffer).await {
            Ok(n) if n > 0 => {
                // Append new data to receive buffer
                receive_buffer.reserve(n);
                for byte in &read_buffer[..n] {
                    if *byte == PACKET_DELIMITER {
                        // Then we've read a complete message (in receive_buffer), so decode and push to RX_CHANNEL
                        match postcard::from_bytes_cobs::<Message>(&mut receive_buffer) {
                            Ok(message) => {
                                sender.send(message).await;
                            }
                            Err(e) => {
                                log::error!("Failed to deserialize message: {:?}", e);
                            }
                        }
                        // Clear receive buffer and start reading again
                        receive_buffer.clear();
                    } else {
                        // Otherwise, byte is part of the message, so add to receive buffer
                        receive_buffer.push(*byte);
                    }
                }
            }
            Ok(_) => {
                // No data available, yield and retry
            }
            Err(e) => {
                // Error reading, log and retry
                log::error!("Error reading from UART. {:?}", e);
            }
        }
        yield_now().await;
    }
}
