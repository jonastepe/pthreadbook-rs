use std::thread;
use std::io::prelude::*;
use std::time::Duration;

fn main() {
    loop {
        let mut line = String::new();

        print!("Alarm> ");
        match std::io::stdout().flush() {
            Ok(()) => {},
            Err(error) => panic!(format!("error while flushing stdout: {}", error)),
        }
        match std::io::stdin().read_line(&mut line) {
            Ok(_) => {},
            Err(error) => panic!(format!("error while reading line: {}", error)),
        }

        let (seconds, message) = line.split_at(line.find(" ").expect("Bad command"));

        let message = message.trim().to_owned();
        let seconds = match seconds.parse::<u16>() {
            Ok(s) => s,
            Err(error) => panic!(format!("failed to parse seconds: {}", error)),
        };

        thread::spawn(move || {
            thread::sleep(Duration::from_secs(seconds as u64));
            println!("({}) {}", seconds, message);
        });
    }
}
