pub mod logger;
pub mod messages;

use embassy_executor::Spawner;
use esp_idf_svc::hal::gpio::{AnyIOPin, AnyInputPin};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::uart::{UartConfig, AsyncUartDriver};
use esp_idf_svc::hal::units::Hertz;
use smart_leds::{RGB8, SmartLedsWrite};
use ws2812_esp32_rmt_driver::driver::color::LedPixelColorGrb24;
use ws2812_esp32_rmt_driver::LedPixelEsp32Rmt;
use logger::SerialLogger;
use common::message::Message;

const NUM_LEDS: usize = 513;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // It is necessary to call this function once. Otherwise, some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    let peripherals = Peripherals::take().unwrap();
    
    // Create UART driver for UART1
    let uart_config = UartConfig::default().baudrate(Hertz(115_200));
    let uart = AsyncUartDriver::new(
        peripherals.uart1,
        peripherals.pins.gpio4,
        peripherals.pins.gpio3,
        Option::<AnyInputPin>::None,
        Option::<AnyIOPin>::None,
        &uart_config
    ).unwrap();
    
    // Split UART into TX and RX components and spawn tasks
    let uart = Box::leak(Box::new(uart));
    let (tx, rx) = uart.split();
    spawner.spawn(messages::tx_task(tx)).unwrap();
    spawner.spawn(messages::rx_task(rx)).unwrap();
    
    // Initialize logger
    SerialLogger::new().init(log::LevelFilter::Info).unwrap();

    log::info!("Initializing...");
    
    let led_pin = peripherals.pins.gpio10;
    let channel = peripherals.rmt.channel0;
    let mut led_driver = LedPixelEsp32Rmt::<RGB8, LedPixelColorGrb24>::new(channel, led_pin).unwrap();

    // Clear LEDs
    let pixels: Vec<RGB8> = std::iter::repeat(RGB8::new(0, 0, 0)).take(NUM_LEDS).collect();
    if let Err(err) = led_driver.write(pixels) {
        log::error!("Failed to clear LEDs: {:?}", err);
    }

    log::info!("System ready, entering main loop...");

    // Get reference to log channel for processing log messages
    let log_channel = logger::SerialLogger::channel();
    let log_receiver = log_channel.receiver();
    
    // Get receivers/senders for UART channels
    let rx_receiver = messages::RX_CHANNEL.receiver();
    let tx_sender = messages::TX_CHANNEL.sender();

    // Main loop: continuously read messages from channel and process log messages
    loop {
        // Process log messages from channel (non-blocking) - send them to UART TX
        while let Ok(log_message) = log_receiver.try_receive() {
            let _ = tx_sender.try_send(log_message);
        }

        // Try to receive a message from UART (non-blocking)
        match rx_receiver.try_receive() {
            Ok(message) => {
                // Handle received message
                match message {
                    Message::Heartbeat => {
                        // Respond with heartbeat
                        log::info!("Received heartbeat for some reason {:?}", message);
                        let _ = tx_sender.try_send(Message::Heartbeat);
                    }
                    Message::SetLeds(payload) => {
                        log::info!("Received SetLeds command with {} LEDs", payload.leds.len());
                        // Convert RGB values to RGB8 and write to LEDs
                        let pixels: Vec<RGB8> = payload.leds
                            .iter()
                            .map(|rgb| RGB8 {
                                r: rgb.r,
                                g: rgb.g,
                                b: rgb.b,
                            })
                            .collect();
                        
                        if pixels.len() == NUM_LEDS {
                            if let Err(e) = led_driver.write(pixels) {
                                log::error!("Failed to write LEDs: {:?}", e);
                            }
                        } else {
                            log::warn!("Received {} LEDs, expected {}", pixels.len(), NUM_LEDS);
                        }
                    }
                    msg => {
                        log::warn!("Received unexpected message: {:?}", msg);
                    }
                }
            }
            Err(embassy_sync::channel::TryReceiveError::Empty) => {
                // No message available, continue
            }
        }
        
        // Small delay to yield CPU time and prevent watchdog timeout
        embassy_time::Timer::after(embassy_time::Duration::from_millis(10)).await;
    }
}
