mod messages;

use common::message::{Message, Rgb, SetLedsPayload};
use messages::MessageHandler;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Connecting to serial port /dev/ttyS3 at 115200 baud...");
    
    // Create message handler connected to /dev/ttyS3 at 115200 baud
    let message_handler = MessageHandler::new("/dev/ttyS3", 115200)?;
    
    println!("Connected! Starting main loop...");

    let mut red: u8 = 100;
    let mut since_message: u8 = 0;

    // Main loop: continuously send and receive messages
    loop {
        // Try to receive a message (non-blocking)
        match message_handler.try_receive() {
            Ok(Some(message)) => {
                // Handle received message
                match message {
                    Message::Heartbeat => {
                        println!("Received heartbeat");
                    }
                    Message::Log(payload) => {
                        // Display log messages from the firmware
                        println!("[{}] {}", payload.level(), payload.content);
                    }
                    msg => {
                        println!("Received unexpected message: {:?}", msg);
                    }
                }
            }
            Ok(None) => {
                // No message available, continue
            }
            Err(e) => {
                eprintln!("Error receiving message: {}", e);
            }
        }

        // Small delay to avoid busy waiting
        std::thread::sleep(Duration::from_millis(10));
        since_message += 1;
        if since_message >= 100 {
            since_message = 0;
            let message = Message::Heartbeat;
            println!("Sending heartbeat");
            message_handler.send(&message)?;
        }
        // if red >= 255 {
        //     red = 0;
        // } else {
        //     red += 1;
        // }
        // // Send a SetLeds message with the red value
        // let pixels: Vec<Rgb> = std::iter::repeat(Rgb::new(red, 0, 0)).take(513).collect();
        // let message = Message::SetLeds(SetLedsPayload { leds: pixels });
        // message_handler.send(&message)?;
    }
}
