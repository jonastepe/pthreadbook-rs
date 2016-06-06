use std::io::prelude::*;
use std::time::{Instant, Duration};
use std::cmp::Ord;
use std::thread;
use std::sync::{Mutex, Arc};

struct Alarm {
    seconds: Duration,
    time: Instant,
    message: String,
}

impl Alarm {
    fn new(s: u64, m: String) -> Self {
        let s = Duration::from_secs(s);
        Alarm {
            seconds: s,
            time: Instant::now() + s,
            message: m,
        }
    }
}

type AlarmList = Arc<Mutex<Vec<Alarm>>>;

fn start_alarm_thread(alarm_list: AlarmList) {
    thread::spawn(move || {
        loop {
            let alarm = alarm_list.lock().unwrap().pop();
            match alarm {
                Some(a) => {
                    let now = Instant::now();
                    if a.time <= now {
                        thread::yield_now();
                    } else {
                        thread::sleep(a.time.duration_since(now));
                    }

                    println!("({}) \"{}\"", a.seconds.as_secs(), a.message);
                },
                None => thread::sleep(Duration::from_secs(1))
            }
        }
    });
}

fn main() {
    
    let alarms = Arc::new(Mutex::new(Vec::<Alarm>::new()));

    start_alarm_thread(alarms.clone());
    
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
        let seconds = match seconds.parse::<u64>() {
            Ok(s) => s,
            Err(error) => panic!(format!("failed to parse seconds: {}", error)),
        };

        match alarms.lock() {
            Ok(mut alarm_vec) => {
                alarm_vec.push(Alarm::new(seconds, message));
                alarm_vec.sort_by(|a, b| b.time.cmp(&a.time));
            },
            Err(e) => panic!(format!("failed to lock mutex: {}", e)),
        }
    }
}

