use std::sync::{Mutex,Condvar,Arc};
use std::thread;

struct Barrier {
    threshold: usize,
    state: Mutex<BarrierState>,
    open: Condvar,
}

impl Barrier {
    fn new(threshold: usize) -> Self {
        Barrier {
            threshold: threshold,
            open: Condvar::new(),
            state: Mutex::new(BarrierState::new(threshold)),
        }
    }

    fn wait(&self) -> bool {
        let ret;
        
        match self.state.lock() {
            Err(e) => panic!(format!("Locking failed: {}", e)),
            Ok(mut state) => {
                if !state.valid {
                    panic!("Waiting on invalid Barrier");
                }

                let current_cycle = state.cycle;
                state.counter -= 1;
                
                if state.counter <= 0 {
                    state.cycle = !state.cycle;
                    state.counter = self.threshold;
                    self.open.notify_all();
                    ret = true;
                } else {
                    while current_cycle == state.cycle {
                        state = self.open.wait(state).unwrap();
                    }
                    ret = false;
                }
            },
        }

        ret
    }
}

struct BarrierState {
    counter: usize,
    cycle: bool,
    valid: bool,
}

impl BarrierState {
    fn new(counter: usize) -> Self {
        BarrierState {
            counter: counter,
            cycle: true,
            valid: true,
        }
    }
}

const ARRAY_SIZE: usize = 6;
const THREADS: usize = 5;
const OUTLOOPS: usize = 10;
const INLOOPS: usize = 1000;

struct ThreadContext {
    array: Mutex<ThreadArray>,
    barrier: Arc<Barrier>,
}

impl ThreadContext {
    fn new(increment: u32, barrier: Arc<Barrier>) -> Self {
        ThreadContext {
            array: Mutex::new(ThreadArray::new(increment)),
            barrier: barrier,
        }
    }
}

struct ThreadArray {
    data: [u32; ARRAY_SIZE],
    increment: u32,
}

impl ThreadArray {
    fn new(increment: u32) -> Self {
        let mut array = ThreadArray {
            data: [0; ARRAY_SIZE],
            increment: increment,
        };

        for i in 0..array.data.len() {
            array.data[i] = (i + 1) as u32;
        }

        array
    }
}

fn main() {
    let mut handles = Vec::with_capacity(THREADS);
    let barrier = Arc::new(Barrier::new(THREADS));
    let mut contexts = Vec::with_capacity(THREADS);
    
    for i in 0..THREADS {
        contexts.push(ThreadContext::new(i as u32, barrier.clone()));
    }

    let contexts = Arc::new(contexts);
    for i in 0..contexts.len() {
        let thread_contexts = contexts.clone();

        handles.push(thread::spawn(move || {
            thread_routine(thread_contexts, i as usize);
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    for p in contexts.iter().enumerate() {
        let thread_num = p.0;
        let ctx = p.1;
        
        match ctx.array.lock() {
            Err(_) => panic!(format!("Failed to lock data mutex to print results")),
            Ok(array) => {
                print!("{}", format!("{:02}: ({}) ", thread_num, array.increment));

                for i in 0..array.data.len() {
                    print!("{}", format!("{:10} ", array.data[i]));
                }
                print!("\n");
            },
        }
    }
}

fn thread_routine(contexts: Arc<Vec<ThreadContext>>, thread_num: usize) {
    let my_ctx = &contexts[thread_num];

    for _ in 0..OUTLOOPS {
        my_ctx.barrier.wait();

        match my_ctx.array.lock() {
            Err(e) => panic!(format!("Unable to lock mutex in inner loop: {}", e)),
            Ok(mut array) => {
                for _ in 0..INLOOPS {
                    for counter in 0..array.data.len() {
                        array.data[counter] += array.increment;
                    }
                }
            },
        }

        if my_ctx.barrier.wait() {
            for ctx in contexts.iter() {
                match ctx.array.lock() {
                    Err(e) => panic!(format!("Unable to lock mutex to increment: {}", e)),
                    Ok(mut array) => {
                        array.increment += 1;
                    },
                }
            }
        }
    }
}
