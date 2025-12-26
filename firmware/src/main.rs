#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

pub mod logger;
pub mod messages;

use embassy_executor::Spawner;
use esp_backtrace as _;
use esp_hal::time::Rate;
use esp_hal::uart;
use esp_hal::rmt::Rmt;
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::uart::{AtCmdConfig, RxConfig, Uart};
use esp_hal_smartled::{SmartLedsAdapterAsync, buffer_size_async};
use smart_leds::{RGB8, SmartLedsWriteAsync, gamma};
// use logger::SerialLogger;
use common::message::Message;
use alloc::vec::Vec;

use crate::messages::{FIFO_FULL_THRESHOLD, PACKET_DELIMITER};

extern crate alloc;


const NUM_LEDS: usize = 513;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 65536);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    log::info!("Embassy initialized!");


    // Create RMT led driver
    let rmt: Rmt<'_, esp_hal::Async> = Rmt::new(peripherals.RMT, Rate::from_mhz(80))
        .expect("Failed to initialize RMT")
        .into_async();

    let rmt_channel = rmt.channel0;
    let mut rmt_buffer = [esp_hal::rmt::PulseCode::default(); buffer_size_async(NUM_LEDS)];

    let mut led_driver = SmartLedsAdapterAsync::new(rmt_channel, peripherals.GPIO10, &mut rmt_buffer);

    // Clear LEDs
    let pixels: Vec<RGB8> = core::iter::repeat(RGB8::new(0, 0, 0)).take(NUM_LEDS).collect();
    if let Err(e) = led_driver.write(pixels).await {
        log::error!("Failed to write LEDs: {:?}", e);
    }

    log::info!("RMT led driver initialized");

    
    // Create UART driver for UART0
    let config = uart::Config::default()
        .with_rx(RxConfig::default().with_fifo_full_threshold(FIFO_FULL_THRESHOLD as u16));

    let mut uart0 = Uart::new(peripherals.UART0, config)
        .expect("Failed to initialize UART")
        .into_async();
    uart0.set_at_cmd(AtCmdConfig::default().with_cmd_char(PACKET_DELIMITER));

    let (rx, tx) = uart0.split();

    log::info!("UART driver initialized");

    // Start embassy tasks to send and receive messages over UART
    spawner.spawn(messages::tx_task(tx)).unwrap();
    spawner.spawn(messages::rx_task(rx)).unwrap();
    
    // Initialize logger
    // SerialLogger::new().init(log::LevelFilter::Info).unwrap();

    log::info!("System initialized, entering main loop...");
    
    // Get receivers/senders for UART channels
    let message_receiver = messages::RX_CHANNEL.receiver();
    let message_sender = messages::TX_CHANNEL.sender();

    // Main loop: continuously read messages from channel and process log messages
    loop {
        // Try to receive a message from UART (non-blocking)
        let message = message_receiver.receive().await;
        match message {
            Message::Heartbeat => {
                // Respond with heartbeat
                log::info!("Received heartbeat for some reason {:?}", message);
                let _ = message_sender.try_send(Message::Heartbeat);
            }
            Message::SetLeds(payload) => {
                log::info!("Received SetLeds command with {} LEDs", payload.leds.len());
                // Convert RGB values to RGB8 and write to LEDs
                let pixels: Vec<RGB8> = gamma(payload.leds
                    .iter()
                    .map(|rgb| RGB8 {
                        r: rgb.r,
                        g: rgb.g,
                        b: rgb.b,
                    }))
                    .collect();
                
                if pixels.len() == NUM_LEDS {
                    if let Err(e) = led_driver.write(pixels).await {
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
}
