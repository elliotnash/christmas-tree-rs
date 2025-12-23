use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::sys::esp_random;
use smart_leds::hsv::{hsv2rgb, Hsv};
use smart_leds::{SmartLedsWrite, RGB8};
use std::thread::sleep;
use std::time::Duration;
use ws2812_esp32_rmt_driver::driver::color::LedPixelColorGrb24;
use ws2812_esp32_rmt_driver::LedPixelEsp32Rmt;

const NUM_LEDS: usize = 513;

fn main() -> ! {
    // It is necessary to call this function once. Otherwise, some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Initializing...");

    let peripherals = Peripherals::take().unwrap();
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

        hue = hue.wrapping_add(1);
    }
}
