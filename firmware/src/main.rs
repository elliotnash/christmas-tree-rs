pub mod logger;

use esp_idf_svc::hal::gpio::{AnyIOPin, AnyInputPin};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::sys::esp_random;
use esp_idf_svc::hal::uart::{UartConfig, UartDriver};
use smart_leds::hsv::{hsv2rgb, Hsv};
use smart_leds::{SmartLedsWrite, RGB8};
use std::thread::sleep;
use std::time::Duration;
use ws2812_esp32_rmt_driver::driver::color::LedPixelColorGrb24;
use ws2812_esp32_rmt_driver::LedPixelEsp32Rmt;
use logger::SerialLogger;

const NUM_LEDS: usize = 513;

fn main() -> ! {
    // It is necessary to call this function once. Otherwise, some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();
    
    // Create UART driver
    let uart_config = UartConfig::default();
    let uart = UartDriver::new(
        peripherals.uart1,
        peripherals.pins.gpio4,
        peripherals.pins.gpio3,
        Option::<AnyInputPin>::None,
        Option::<AnyIOPin>::None,
        &uart_config
    ).unwrap();
    SerialLogger::new(uart).init(log::LevelFilter::Info).unwrap();

    log::info!("Initializing...");
    
    let led_pin = peripherals.pins.gpio10;
    let channel = peripherals.rmt.channel0;
    let mut led_driver = LedPixelEsp32Rmt::<RGB8, LedPixelColorGrb24>::new(channel, led_pin).unwrap();

    let mut hue = unsafe { esp_random() } as u8;
    loop {
        let pixels = std::iter::repeat(hsv2rgb(Hsv {
            hue,
            sat: 255,
            val: 50,
        }))
        .take(NUM_LEDS);
        led_driver.write(pixels).unwrap();

        sleep(Duration::from_millis(10));

        log::info!("Hue: {}", hue);

        println!("THIS IS PRINTLN");

        hue = hue.wrapping_add(1);
    }
}
