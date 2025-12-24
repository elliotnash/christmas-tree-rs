pub mod logger;
pub mod messages;

use esp_idf_svc::hal::gpio::{AnyIOPin, AnyInputPin};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::uart::{UartConfig, UartDriver};
use esp_idf_svc::hal::units::Hertz;
use smart_leds::{RGB8, SmartLedsWrite};
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;
use ws2812_esp32_rmt_driver::driver::color::LedPixelColorGrb24;
use ws2812_esp32_rmt_driver::LedPixelEsp32Rmt;
use logger::SerialLogger;
use messages::MessageHandler;
use common::message::Message;

const NUM_LEDS: usize = 513;

fn main() -> ! {
    // It is necessary to call this function once. Otherwise, some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    // esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();
    
    // Create UART driver for UART1
    let uart_config = UartConfig::default().baudrate(Hertz(115_200));
    let uart = UartDriver::new(
        peripherals.uart1,
        peripherals.pins.gpio4,
        peripherals.pins.gpio3,
        Option::<AnyInputPin>::None,
        Option::<AnyIOPin>::None,
        &uart_config
    ).unwrap();
    
    // Create MessageHandler for UART communication
    let message_handler = Arc::new(MessageHandler::new(uart));
    
    // Initialize logger with MessageHandler
    SerialLogger::new(message_handler.clone()).init(log::LevelFilter::Info).unwrap();

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

    // Main loop: continuously read messages from UART
    loop {
        // Try to receive a message (non-blocking)
        match message_handler.try_receive() {
            Ok(Some(message)) => {
                // Handle received message
                match message {
                    Message::Heartbeat => {
                        // Respond with heartbeat
                        log::info!("Received heartbeat for some reason {:?}", message);
                        let _ = message_handler.send(&Message::Heartbeat);
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
            Ok(None) => {
                // No message available, continue
            }
            Err(e) => {
                log::warn!("Error receiving message: {}", e);
            }
        }
        
        // Small delay to yield CPU time and prevent watchdog timeout
        sleep(Duration::from_millis(10));
    }
}
