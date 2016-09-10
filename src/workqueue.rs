extern crate rand;

use std::sync::{Mutex, Condvar, Arc};
use std::collections::VecDeque;
use std::fmt;
use std::thread;

#[derive(Debug)]
enum WorkqueueError {
    Quit,
}

impl fmt::Display for WorkqueueError {    
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            &WorkqueueError::Quit => write!(f, "Workqueue set to quit."),
        }
    }
}

impl std::error::Error for WorkqueueError {
    fn description(&self) -> &str {
        match self {
            &WorkqueueError::Quit => "It's illegal to use a Workqueue that's set to quit.",
        }
    }

    fn cause(&self) -> Option<&std::error::Error> {
        None
    }
}

struct RawWorkqueue<T, F: ?Sized> {
    state: Mutex<WorkqueueState<T>>,
    routine: Box<F>,
    work_present: Condvar,
}

impl<T, F: ?Sized> RawWorkqueue<T, F> {
    fn new(routine: Box<F>) -> Self {
        RawWorkqueue {
            state: Mutex::new(WorkqueueState::new()),
            routine: routine,
            work_present: Condvar::new(),
        }
    }
}

struct Workqueue<T, F: ?Sized> {
    inner: Arc<RawWorkqueue<T, F>>,
    parallelism: usize,
}

impl<T, F: ?Sized> Workqueue<T, F> {
    fn new(routine: Box<F>, parallelism: usize) -> Self {
        Workqueue {
            inner: Arc::new(RawWorkqueue::new(routine)),
            parallelism: parallelism,
        }
    }
}

impl<T, F: ?Sized> Workqueue<T, F>
    where F: Fn(T) -> T + Send + Sync + 'static,
          T: Clone + Send + Sync + 'static
{
    fn add_task(&self, task: T) -> Result<(), WorkqueueError> {

        let mut start_new_worker = false;
        
        match self.inner.state.lock() {
            Err(e) => panic!("Unable to lock workqueue state. {}", e),
            Ok(mut state) => {
                if state.quit {
                    return Err(WorkqueueError::Quit);
                }

                state.tasks.push_back(task);

                if state.idle_counter > 0 {
                    self.inner.work_present.notify_one();
                } else if state.thread_counter < self.parallelism {
                    start_new_worker = true;
                }
            }
        }

        if start_new_worker {
            self.new_worker();
        }

        Ok(())
    }

    fn new_worker(&self) {
        match self.inner.state.lock() {
            Err(e) => panic!("Unable to lock workqueue to start new worker: {}", e),
            Ok(mut state) => {
                state.thread_counter += 1;
            },
        }
        
        let workqueue = self.inner.clone();
        
        thread::spawn(move || {
            Workqueue::worker_routine(workqueue)
        });
    }

    fn quit(&self) -> Result<Vec<Vec<T>>, WorkqueueError> {
        match self.inner.state.lock() {
            Err(e) => panic!("Failed to wait on workqueue quit. {}", e),
            Ok(mut state) => {
                state.quit = true;

                while state.thread_counter > 0 {
                    state = self.inner.work_present.wait(state).unwrap();
                }

                Ok(state.completed.clone())
            },
        }
    }

    fn worker_routine(workqueue: Arc<RawWorkqueue<T, F>>) {
        let mut tasks_completed = vec![];
        
        loop {

            let mut timedout = false;
            let should_quit;
            
            let task = match workqueue.state.lock() {
                Err(e) => panic!("Unable to start new worker thread: {}", e),
                Ok(mut state) => {

                    while state.tasks.is_empty() && !state.quit && !timedout {
                        state.idle_counter += 1;

                        state = match workqueue.work_present.wait_timeout(
                            state,
                            std::time::Duration::from_secs(2)
                        ) {
                            Err(e) => panic!("Failed a timed wait: {}", e),
                            Ok((mut state, timeout)) => {
                                if timeout.timed_out() {
                                    timedout = true;
                                }
                                state.idle_counter -= 1;
                                state
                            },
                        };
                    }

                    should_quit = state.quit;
                    state.tasks.pop_front()
                },
            };

            match task {
                None if timedout || should_quit => {
                    match workqueue.state.lock() {
                        Err(e) => panic!("Failed to get lock to decrease thread count: {}", e),
                        Ok(mut state) => {
                            state.thread_counter -= 1;
                            if state.thread_counter == 0 {
                                workqueue.work_present.notify_all();
                            }
                            break;
                        },
                    }
                },
                Some(t) => {
                    tasks_completed.push((workqueue.routine)(t));
                },
                _ => unreachable!(),
            }
        }

        match workqueue.state.lock() {
            Err(e) => panic!("Failed to lock workqueue after finishing worker_routine: {}", e),
            Ok(mut state) => {
                state.completed.push(tasks_completed);
            },
        }
    }
}

struct WorkqueueState<T> {
    quit: bool,
    thread_counter: usize,
    idle_counter: usize,
    tasks: VecDeque<T>,
    completed: Vec<Vec<T>>,
}

impl<T> WorkqueueState<T> {
    fn new() -> Self {
        WorkqueueState {
            quit: false,
            thread_counter: 0,
            idle_counter: 0,
            tasks: VecDeque::new(),
            completed: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
struct Power {
    value: u64,
    power: u64,
}

impl Power {
    fn new() -> Self {
        Power {
            value: rand::random::<u64>() % 20,
            power: rand::random::<u64>() % 7,
        }
    }
}

const ITERATIONS: usize = 25;

fn test_workqueue<F: ?Sized>(workqueue: Arc<Workqueue<Power, F>>)
    where F: Fn(Power) -> Power,
          F: Send + Sync + 'static
{
    for _ in 0..ITERATIONS {
        match workqueue.add_task(Power::new()) {
            Err(e) => panic!("Failed to add task to workqueue. {}", e),
            Ok(_) => {},
        }

        thread::sleep(std::time::Duration::from_millis(250));
    }
}

fn main() {
    let wq: Workqueue<Power, _> = Workqueue::new(Box::new(move |p: Power| {
        let mut _sum = p.value;
        for _ in 1..p.power {
            _sum *= p.value;
        }
        p
    }), 4);
    let wq = Arc::new(wq);

    let thread_wq = wq.clone();
    
    let handle = thread::spawn(move || {
        test_workqueue(thread_wq);
    });
    
    test_workqueue(wq.clone());

    handle.join().unwrap();
    
    let result = match wq.quit() {
        Err(e) => panic!("Workqueue failed: {}", e),
        Ok(result) => result,
    }.into_iter().enumerate();

    for (i, per_worker) in result {
        println!("worker {:2}, calculated {} powers", i, per_worker.len());
    }
}
