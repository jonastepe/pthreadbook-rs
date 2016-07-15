use std::io::prelude::*;
use std::time::{Instant, Duration};
use std::sync::{Mutex, Arc, Condvar};
use std::thread;
use std::cmp::Ordering;

struct Alarm {
    time: Instant,
    seconds: u64,
    message: String,
}

struct AlarmBacklog {
    backlog: Vec<Alarm>,
    next: Option<Alarm>,
}

impl AlarmBacklog {
    fn new() -> Self {
        AlarmBacklog { backlog: Vec::new(), next: None }
    }

    fn prioritize_new_alarm(&mut self, alarm: Alarm) -> InsertionResult {
        let mut result = InsertionResult::NoChange;
        
        match self.next.take() {    
            Some(next) => {
                let (earlier, later) = if next.time > alarm.time {
                    result = InsertionResult::NextChanged;
                    (alarm, next)
                } else {
                    (next, alarm)
                };
                self.next = Some(earlier);
                self.backlog.push(later);
                self.backlog.sort_by(|a, b| std::cmp::Ord::cmp(&a.time, &b.time));
            },
            None => {
                self.next = Some(alarm);
                result = InsertionResult::NextChanged;
            },
        }

        result
    }

    fn empty(&self) -> bool {
        self.next.is_none()
    }

    fn extract_next(&mut self) -> Option<Alarm> {
        let next = self.next.take();
        self.next = self.backlog.pop();
        next
    }

    fn cmp_next_to_other_time(&self, i: &Instant) -> Ordering {
        match self.next {
            Some(Alarm { time: ref t, .. }) => Ord::cmp(t, i),
            None => Ordering::Greater,
        }
    }
}

#[derive(PartialEq, Debug)]
enum InsertionResult {
    NoChange,
    NextChanged,
}

fn main() {

    let backlog = Arc::new((Mutex::new(AlarmBacklog::new()), Condvar::new()));

    let waiter_backlog = backlog.clone();

    thread::spawn(move || {
        match waiter_backlog.0.lock() {
            Ok(mut backlog) => {
                loop {

                    // wait until there is a new alarm to process
                    while backlog.empty() {
                        match waiter_backlog.1.wait(backlog) {
                            Ok(guard) => backlog = guard,
                            Err(e) => panic!(format!("error after waking from normal wait: {}", e)),
                        }
                    }

                    // I know there is a current alarm since that was the predicate.
                    let alarm = backlog.extract_next().unwrap();
                    
                    if alarm.time > Instant::now() {
                        let mut expired = false;
                        
                        // wait for the alarm to expire
                        while backlog.cmp_next_to_other_time(&alarm.time) == Ordering::Greater && !expired {
                            let wait_time = alarm.time - Instant::now();
                            backlog = match waiter_backlog.1.wait_timeout(backlog, wait_time) {
                                Ok((guard, timeout)) => {
                                    if timeout.timed_out() {
                                        expired = true;
                                    }
                                    
                                    guard
                                },
                                Err(e) => panic!(format!("timed wait failed: {}", e)),
                            }
                        }

                        // a new alarm with an earlier timeout was inserted?
                        if !expired {
                            backlog.prioritize_new_alarm(alarm);
                        } else {
                            println!("({}) {}", alarm.seconds, alarm.message);
                        }
                    } else {
                        println!("({}) {}", alarm.seconds, alarm.message);
                    }
                }
            },
            Err(e) => panic!(format!("error while trying to lock mutex in waiter thread: {}", e)),
        };
    });
    
    loop {
        let mut line = String::new();

        print!("Alarm> ");
        match std::io::stdout().flush() {
            Ok(()) => {},
            Err(e) => panic!(format!("error while flushing stdout: {}", e)),
        }
        match std::io::stdin().read_line(&mut line) {
            Ok(_) => {},
            Err(e) => panic!(format!("error while reading line: {}", e)),
        }

        let (seconds, message) = line.split_at(line.find(" ").expect("Bad command"));
        let message = message.trim().to_owned();
        let seconds = match seconds.parse::<u64>() {
            Ok(s) => s,
            Err(e) => panic!(format!("failed to parse seconds: {}", e)),
        };

        match backlog.0.lock() {
            Ok(mut guard) => {
                guard.prioritize_new_alarm(Alarm {
                    time: Instant::now() + Duration::from_secs(seconds),
                    seconds: seconds,
                    message: message,
                });
                backlog.1.notify_one();
            },
            Err(e) => panic!(format!("failed to lock mutex in main thread: {}", e)),
        }
    }
}

#[cfg(test)]
mod test {
    use super::{AlarmBacklog, Alarm, InsertionResult};
    use std::time::{Instant, Duration};
    
    #[test]
    fn prioritize_first_alarm() {
        let mut backlog = AlarmBacklog::new();
        let result = backlog.prioritize_new_alarm(
            Alarm {
                time: Instant::now(),
                seconds: 10,
                message: "Rust".to_string()
        });

        assert_eq!(result, InsertionResult::NextChanged);
    }

    #[test]
    fn prioritize_two_alarms_no_change() {
        let mut backlog = AlarmBacklog::new();

        let earlier = Alarm { time: Instant::now(), seconds: 0, message: "C".to_string() };
        let later = Alarm { time: Instant::now() + Duration::from_secs(10), seconds: 0, message: "Rust".to_string() };

        let result = backlog.prioritize_new_alarm(earlier);
        assert_eq!(result, InsertionResult::NextChanged);

        let result = backlog.prioritize_new_alarm(later);
        assert_eq!(result, InsertionResult::NoChange);
    }

    #[test]
    fn prioritize_two_alarm_next_changed() {
        let mut backlog = AlarmBacklog::new();

        let earlier = Alarm { time: Instant::now(), seconds: 0, message: "C".to_string() };
        let later = Alarm { time: Instant::now() + Duration::from_secs(10), seconds: 0, message: "Rust".to_string() };

        let result = backlog.prioritize_new_alarm(later);
        assert_eq!(result, InsertionResult::NextChanged);

        let result = backlog.prioritize_new_alarm(earlier);
        assert_eq!(result, InsertionResult::NextChanged);
    }

    #[test]
    fn extract_alarms() {
        let mut backlog = AlarmBacklog::new();
        assert!(backlog.extract_next().is_none());

        backlog.prioritize_new_alarm(Alarm { time: Instant::now() + Duration::from_secs(10), seconds: 10, message: "Rust".to_string() });
        backlog.prioritize_new_alarm(Alarm { time: Instant::now(), seconds: 0, message: "C".to_string() });

        assert!(backlog.extract_next().is_some());
        assert!(backlog.extract_next().is_some());
        assert!(backlog.extract_next().is_none());
    }

    #[test]
    fn cmp_next_to_other_time() {
        use std::cmp::Ordering;
        
        let mut backlog = AlarmBacklog::new();
        assert_eq!(backlog.cmp_next_to_other_time(&Instant::now()), Ordering::Greater);

        backlog.prioritize_new_alarm(Alarm {
            time: Instant::now() + Duration::from_secs(10),
            seconds: 10,
            message: "Rust".to_string(),
        });

        assert_eq!(backlog.cmp_next_to_other_time(&(Instant::now() + Duration::from_secs(15))), Ordering::Less);
        assert_eq!(backlog.cmp_next_to_other_time(&(Instant::now() + Duration::from_secs(5))), Ordering::Greater);
    }
}
