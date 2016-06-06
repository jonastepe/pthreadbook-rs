// To achieve vaguely similar behaviour to the C version build
// in release mode (e.g. cargo run --bin trylock --release)

use std::thread;
use std::sync::{Mutex, Arc};
use std::time::{Instant, Duration};

const SPIN: u64 = 10000000000;

fn main() {
    let main_counter = Arc::new(Mutex::new(0u64));
    let end = Instant::now() + Duration::from_secs(10);
    
    let inc_counter = main_counter.clone();
    let counter_thread = thread::spawn(move || {
        while Instant::now() <= end {
            {
                let mut inc_counter = inc_counter.lock().unwrap();
                for _ in 0..SPIN {
                    *inc_counter = *inc_counter + 1;
                }
            }
            thread::sleep(Duration::from_secs(1));
        }

        println!("Counter is {:x}", *inc_counter.lock().unwrap());
    });

    let watch_counter = main_counter.clone();
    let monitor_thread = thread::spawn(move || {
        let mut misses = 0;

        while Instant::now() <= end {
            thread::sleep(Duration::from_secs(3));
            
            match watch_counter.try_lock() {
                Ok(guard) => println!("Counter is {}", *guard / SPIN),
                Err(_) => misses = misses + 1,
            }
        }

        println!("Monitor thread missed update {} times.", misses);
    });

    monitor_thread.join().unwrap();
    counter_thread.join().unwrap();

    println!("The counter is at {}", *main_counter.lock().unwrap() / SPIN);
}
