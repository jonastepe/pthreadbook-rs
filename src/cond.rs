use std::sync::{Arc, Mutex, Condvar};
use std::thread;
use std::time::Duration;

fn main() {
    let hibernation = std::env::args()
        .skip(1).next().unwrap()
        .parse::<u64>().expect("Expected numeric argument for hibernation");

    let data = Arc::new((Mutex::new(0), Condvar::new()));

    let wait_thread_data = data.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(hibernation));

        match wait_thread_data.0.lock() {
            Ok(mut v) => {
                *v += 1;
                wait_thread_data.1.notify_one();
            },
            Err(e) => panic!(format!("wait_thread tried to lock mutex {}", e)),
        };
    });

    match data.0.lock() {
        Ok(mut v) => {
            while *v == 0 {
                let (g, r) = data.1.wait_timeout(v, Duration::from_secs(2)).expect("Timed wait failed");
                v = g;
                if r.timed_out() {
                    println!("Condition wait timed out.");
                    break;
                }
            }
            if *v != 0 {
                println!("Condition was signaled.");
            }
        },
        Err(e) => panic!(format!("main_thread tried to lock mutex {}", e)),
    };
}
