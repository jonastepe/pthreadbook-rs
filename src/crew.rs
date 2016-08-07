use std::path::PathBuf;
use std::sync::{Mutex,Condvar,Arc};
use std::io::{Read,Write};
use std::thread;
use std::fs::{File,read_dir,symlink_metadata};
use std::os::unix::fs::FileTypeExt;

type Messages = Arc<Mutex<Vec<String>>>;

struct WorkItem {
    path: PathBuf,
    search: String,
}

impl WorkItem {
    fn new(p: PathBuf, s: String) -> Self {
        WorkItem { path: p, search: s }
    }
}

struct Crew {
    work: Mutex<(Vec<WorkItem>, usize)>,
    go: Condvar,
    done: Condvar,
}

impl Crew {
    fn new() -> Self {
        Crew {
            work: Mutex::new((Vec::new(), 0)),
            go: Condvar::new(),
            done: Condvar::new(),
        }
    }

    fn start(&self, w: WorkItem) {        
        match self.work.lock() {
            Err(e) => panic!(format!("Error trying to lock mutex in order to start crew: {}", e)),
            Ok(mut guard) => {
                // wait for crew to finish with the old work
                while guard.1 > 0 {
                    guard = self.done.wait(guard).unwrap();
                }

                guard.0.push(w);
                guard.1 += 1;
                self.go.notify_one();

                // wait for the crew to finish with the new work
                while guard.1 > 0 {
                    guard = self.done.wait(guard).unwrap();
                }
            },
        }
    }
}

fn start_crew_work(w: WorkItem, crew_size: usize) {
    let crew = Arc::new(Crew::new());
    let messages = Arc::new(Mutex::new(Vec::new()));

    for i in 0..crew_size {
        let crew = crew.clone();
        let messages = messages.clone();
        
        thread::spawn(move || {
            worker_routine(crew, messages, i);
        });
    }

    // start the crew with the WorkItem. this blocks until the crew is finished
    crew.start(w);

    // print results
    match messages.lock() {
        Err(e) => panic!(format!("Main thread tried to lock message-queue: {}", e)),
        Ok(messages) => {
            for m in messages.iter() {
                println!("{}", m);
            }
        },
    };
}

fn worker_routine(crew: Arc<Crew>,
                  mut messages: Messages,
                  thread_index: usize)
{
    // keep doing work until all is finished
    loop {
        let workitem = match crew.work.lock() {
            Err(e) => panic!(format!("Thread {} tried to lock crew mutex at the start: {}",
                                     thread_index,
                                     e)),
            Ok(mut work) => {
                while work.0.len() == 0 {
                    work = crew.go.wait(work).unwrap();
                }

                // we know there is an element in the work vector
                let workitem = work.0.pop().unwrap();
                workitem
            },
        };

        messages = process_work_item(workitem, messages, thread_index, crew.clone());

        // correct work count. if we reached zero, then we're done
        match crew.work.lock() {
            Err(e) => panic!(format!("Thread {} tried to lock crew mutex after processing: {}",
                                     thread_index,
                                     e)),
            Ok(mut work) => {
                work.1 -= 1;
                if work.1 == 0 {
                    crew.done.notify_all();
                    break;
                }
            },
        }
    }
}

fn process_work_item(w: WorkItem,
                     messages: Messages,
                     thread_index: usize,
                     crew: Arc<Crew>) -> Messages
{
    let mut thread_local_messages = Vec::with_capacity(16);

    let file_type = match symlink_metadata(&w.path) {
        Err(e) => panic!(format!("Thread {} tried to query metadata about file {:?}: {}",
                                 thread_index,
                                 &w.path,
                                 e)),
        Ok(m) => m,
    }.file_type();

    if file_type.is_symlink() {
        thread_local_messages.push(format!("Thread {} found symlink {:?}, not processing.",
                                           thread_index,
                                           &w.path));
    } else if file_type.is_dir() {
        let dir_entries = match read_dir(&w.path) {
            Err(e) => panic!(format!("Thread {} unable to list entries in directory {:?}: {}",
                                     thread_index,
                                     &w.path,
                                     e)),
            Ok(e) => e,
        };

        let new_work_items = dir_entries.map(|result| {
            let entry = match result {
                Err(e) => panic!(format!("Thread {} unable to read directory entry: {}",
                                         thread_index,
                                         e)),
                Ok(e) => e,
            };
            WorkItem::new(entry.path(), w.search.clone())
        }).collect::<Vec<_>>();

        match crew.work.lock() {
            Err(e) => panic!(format!("Thread {} unable to lock mutex to add new work items: {}",
                                     thread_index,
                                     e)),
            Ok(mut work) => {
                work.1 += new_work_items.len();
                work.0.extend(new_work_items);
                crew.go.notify_all();
            },
        }
    } else if file_type.is_file() {
        let mut buffer = String::new();
            
        let mut file = match File::open(&w.path) {
            Err(e) => panic!(format!("Thread {} tried to open file {:?}: {}",
                                     thread_index,
                                     &w.path,
                                     e)),
            Ok(f) => f,
        };

        match file.read_to_string(&mut buffer) {
            Err(e) => panic!(format!("Thread {} tried to read file {:?}: {}",
                                     thread_index,
                                     &w.path,
                                     e)),
            Ok(_) => {},
        }

        if buffer.contains(&w.search) {
            thread_local_messages.push(format!("Thread {} found {:?} in {:?}",
                                               thread_index,
                                               &w.search,
                                               &w.path));
        }
    } else {
        let message = format!("Thread {} could not process file because it is {}.",
                              thread_index,
                              if file_type.is_block_device() {
                                  "a block device"
                              } else if file_type.is_char_device() {
                                  "a char device"
                              } else if file_type.is_fifo() {
                                  "a fifo"
                              } else if file_type.is_socket() {
                                  "a socket"
                              } else {
                                  "unknown"
                              }
        );

        thread_local_messages.push(message);
    }
    
    match messages.lock() {
        Err(e) => panic!(format!("Thread {} could not lock message mutex: {}",
                                 thread_index,
                                 e)),
        Ok(mut m) => m.extend(thread_local_messages),
    }

    messages
}

fn main() {
    let mut args = std::env::args().skip(1);
    let search = match args.next() {
        None => abort_with_usage_message(),
        Some(s) => s,
    };
    let path = match args.next() {
        None => abort_with_usage_message(),
        Some(s) => PathBuf::from(s),
    };

    start_crew_work(WorkItem::new(path, search), 4);
}

fn abort_with_usage_message() -> ! {
    writeln!(&mut std::io::stderr(), "usage: crew search path").unwrap();
    std::process::exit(1)
}
