extern crate libc;

use std::io::prelude::*;
use std::thread;
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

        let message = message.trim();
        let seconds = match seconds.parse::<u16>() {
            Ok(s) => s,
            Err(error) => panic!(format!("failed to parse seconds: {}", error)),
        };

        match unsafe { libc::fork() } {
            -1 => panic!(format!("failed to create child process")),
            0  => {
                println!("child pid: {}", unsafe { libc::getpid() });
                thread::sleep(Duration::from_secs(seconds as u64));
                println!("({}) {}", seconds, message);
                std::process::exit(0);
            },
            _ => {
                loop {
                    match unsafe { libc::waitpid(-1, std::ptr::null_mut(), libc::WNOHANG) } {
                        -1 => panic!(format!("error while waiting for child process")),
                        0  => /* noting to collect */ break,
                        _  => { /* continue looping */ },
                    }
                }
            }
        }
    }
}

