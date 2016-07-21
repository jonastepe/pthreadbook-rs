use std::sync::{Mutex, Condvar, Arc};
use std::thread;
use std::ops::Deref;
use std::io::Write;

struct Stage {
    avail: Condvar,
    ready: Condvar,
    stage_data: Mutex<StageData>,
}

impl Stage {
    fn new(data: u64) -> Self {
        Stage {
            avail: Condvar::new(),
            ready: Condvar::new(),
            stage_data: Mutex::new(StageData { data: data, unprocessed: false }),
        }
    }
}

struct StageData {
    data: u64,
    unprocessed: bool,
}

struct Pipe {
    stages: Vec<Stage>,
    active_count: Mutex<usize>,
}

impl Pipe {
    fn new(stages: usize) -> Self {
        Pipe {
            stages: (0..stages + 1).map(|_| Stage::new(0)).collect(),
            active_count: Mutex::new(0),
        }
    }

    fn head(&self) -> &Stage {
        &self.stages[0]
    }

    fn tail(&self) -> &Stage {
        &self.stages[self.stages.len() - 1]
    }
}

impl Deref for Pipe {
    type Target = [Stage];

    fn deref(&self) -> &Self::Target {
        &self.stages
    }
}

fn worker(pipe: &[Stage], stage_idx: usize) {
    let stage = &pipe[stage_idx];

    match stage.stage_data.lock() {
        Err(e) => panic!(format!("Error trying to lock mutex in worker for stage_index {} : {}",
                         stage_idx,
                         e)),
        Ok(mut guard) => {
            loop {
                while !guard.unprocessed {
                    guard = stage.avail.wait(guard).unwrap();
                }

                send(&pipe[stage_idx + 1], guard.data + 1);
                guard.unprocessed = false;
                stage.ready.notify_one();
            }
        },
    }
}

fn send(target_stage: &Stage, new_data: u64) {
    match target_stage.stage_data.lock() {
        Err(e) => panic!(format!("Error tyring to lock mutex in send: {}", e)),
        Ok(mut guard) => {
            while guard.unprocessed {
                guard = target_stage.ready.wait(guard).unwrap();
            }
            guard.data = new_data;
            guard.unprocessed = true;
            target_stage.avail.notify_one();
        },
    }
}

fn create_pipe(stages: usize) -> Arc<Pipe> {
    assert!(stages > 0);
    let pipe = Arc::new(Pipe::new(stages));

    for i in 0..stages {
        let pipe = pipe.clone();

        thread::spawn(move || {
            worker(&pipe, i);
        });
    }

    pipe
}

fn pipe_start(pipe: &Pipe, data: u64) {
    match pipe.active_count.lock() {
        Err(e) => panic!(format!("Error trying to lock active_count mutex in pipe_start: {}", e)),
        Ok(mut active_count) => {
            *active_count += 1;
        },
    }
    
    send(pipe.head(), data);
}

fn pipe_result(pipe: &Pipe) -> u64 {
    let empty = match pipe.active_count.lock() {
        Err(e) => panic!(format!("Error trying to lock active_count mutex in pipe_result: {}", e)),
        Ok(mut active_count) => {
            if *active_count <= 0 {
                true
            } else {
                *active_count -= 1;
                false
            }
        },
    };

    if empty {
        return 0;
    }

    let tail = pipe.tail();
    match tail.stage_data.lock() {
        Err(e) => panic!(format!("Error trying to lock stage_data mutex in pipe_result: {}", e)),
        Ok(mut stage_data) => {
            while !stage_data.unprocessed {
                stage_data = tail.avail.wait(stage_data).unwrap();
            }
            let result = stage_data.data;
            stage_data.unprocessed = false;
            
            tail.ready.notify_one();
            
            result
        },
    }
}

fn main() {
    let pipe = create_pipe(2);

    println!("Enter integer values, or \"=\" for next result");

    loop {
        let mut buffer = String::with_capacity(128);
        
        print!("Data> ");
        std::io::stdout().flush().expect("Error flushing stdout.");

        match std::io::stdin().read_line(&mut buffer) {
            Err(e) => panic!(format!("Error trying to read line of input: {}", e)),
            Ok(n) => {
                if n > 0 {
                    if buffer.chars().next() == Some('=') {
                        let result = pipe_result(&pipe);
                        if result == 0 {
                            println!("Pipe is empty.")
                        } else {
                            println!("result: {}", result);
                        }
                    } else {
                        let new_data = buffer.trim().parse::<u64>().expect("Error trying to read input as number.");
                        pipe_start(&pipe, new_data);
                    }
                }
            }
        }
    }
}

