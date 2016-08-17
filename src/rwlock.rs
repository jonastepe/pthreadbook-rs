#![feature(optin_builtin_traits)]

extern crate rand;

use std::sync::{Mutex, Condvar, Arc};
use std::ops::{Deref, DerefMut};
use std::thread;
use std::fmt;
use std::cell::UnsafeCell;

struct RWLock<T> {
    state: Mutex<RWLockState>,
    read: Condvar,
    write: Condvar,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send + Sync> Send for RWLock<T> {}
unsafe impl<T: Send + Sync> Sync for RWLock<T> {}

impl<T> RWLock<T> {
    fn new(d: T) -> Self {
        RWLock {
            state: Mutex::new(RWLockState::new()),
            read: Condvar::new(),
            write: Condvar::new(),
            data: UnsafeCell::new(d),
        }
    }

    fn into_inner(self) -> Result<T, &'static str> {
        match self.state.lock() {
            Err(_) => Err("Could not acquire lock reliably to move inner data out."),
            Ok(state) => {
                if !state.w_active
                    && state.r_active == 0
                    && state.w_wait == 0
                    && state.r_wait == 0
                {
                    unsafe { Ok(self.data.into_inner()) }
                } else {
                    Err("Cannot take data out of lock, since it's in use.")
                }
            },
        }
    }

    fn read(&self) -> Result<RWLockReadGuard<T>, &'static str> {
        match self.state.lock() {
            Err(_) => Err("Failed to lock for read."),
            Ok(mut state) => {
                while state.w_active {
                    state.r_wait += 1;
                    state = self.read.wait(state).unwrap();
                    state.r_wait -= 1;
                }

                state.r_active += 1;
                Ok(RWLockReadGuard::new(self))
            }
        }
    }

    fn try_read(&self) -> Result<RWLockReadGuard<T>, &'static str> {
        match self.state.lock() {
            Err(_) => Err("Failed to lock for read."),
            Ok(mut state) => {
                if state.w_active {
                    Err("Lock in write state.")
                } else {
                    state.r_active += 1;
                    Ok(RWLockReadGuard::new(self))
                }
            },
        }
    }

    fn write(&self) -> Result<RWLockWriteGuard<T>, &'static str> {
        match self.state.lock() {
            Err(_) => Err("Failed to lock for write."),
            Ok(mut state) => {
                while state.r_active > 0 || state.w_active {
                    state.w_wait += 1;
                    state = self.write.wait(state).unwrap();
                    state.w_wait -= 1;
                }

                state.w_active = true;
                Ok(RWLockWriteGuard::new(self))
            }
        }
    }

    fn try_write(&self) -> Result<RWLockWriteGuard<T>, &'static str> {
        match self.state.lock() {
            Err(_) => Err("Failed to lock for write."),
            Ok(mut state) => {
                if state.r_active > 0 || state.w_active {
                    Err("Lock busy")
                } else {
                    state.w_active = true;
                    Ok(RWLockWriteGuard::new(self))
                }
            },
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for RWLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.state.lock() {
            Err(_) => write!(f, "RWLock locked."),
            Ok(state) => {
                let s = *state;
                write!(f, "RWLock {{ data: {:?}, state: {:?} }}", self.data, s)
            },
        }
    }
}

#[derive(Debug, Copy, Clone)]
struct RWLockState {
    r_active: usize,
    w_active: bool,
    r_wait: usize,
    w_wait: usize,
}

impl RWLockState {
    fn new() -> Self {
        RWLockState {
            r_active: 0,
            w_active: false,
            r_wait: 0,
            w_wait: 0,
        }
    }
}

struct RWLockReadGuard<'a, T: 'a> {
    rwlock: &'a RWLock<T>,
}

impl<'a, T> !Send for RWLockReadGuard<'a, T> {}

impl<'a, T> Deref for RWLockReadGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.rwlock.data.get() }
    }
}

impl<'a, T> Drop for RWLockReadGuard<'a, T> {
    fn drop(&mut self) {
        match self.rwlock.state.lock() {
            Err(e) => panic!("Unable to acquire lock in drop handler for RWLockReadGuard: {}", e),
            Ok(mut state) => {
                state.r_active -= 1;
                
                if state.r_active == 0 && state.w_wait > 0 {
                    self.rwlock.write.notify_one();
                }
            },
        }
    }
}

impl<'a, T> RWLockReadGuard<'a, T> {
    fn new(rwlock: &'a RWLock<T>) -> RWLockReadGuard<'a, T> {
        RWLockReadGuard { rwlock: rwlock }
    }
}

struct RWLockWriteGuard<'a, T: 'a> {
    rwlock: &'a RWLock<T>,
}

impl<'a, T> !Send for RWLockWriteGuard<'a, T> {}

impl<'a, T> Drop for RWLockWriteGuard<'a, T> {
    fn drop(&mut self) {
        match self.rwlock.state.lock() {
            Err(e) => panic!("Unable to acquire lock in drop handler for RWLockWriteGuard: {}", e),
            Ok(mut state) => {
                state.w_active = false;

                if state.r_wait > 0 {
                    self.rwlock.read.notify_all();
                } else if state.w_wait > 0 {
                    self.rwlock.write.notify_one();
                }
            },
        }
    }
}

impl<'a, T> Deref for RWLockWriteGuard<'a, T> {
    type Target = T;
    
    fn deref(&self) -> &T {
        unsafe { &*self.rwlock.data.get() }
    }
}

impl<'a, T> DerefMut for RWLockWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.rwlock.data.get() }
    }
}

impl<'a, T> RWLockWriteGuard<'a, T> {
    fn new(rwlock: &'a RWLock<T>) -> RWLockWriteGuard<'a, T> {
        RWLockWriteGuard { rwlock: rwlock }
    }
}

const THREADS: usize = 5;
const DATASIZE: usize = 15;
const ITERATIONS: usize = 10000;

#[derive(Debug)]
struct ThreadData {
    thread_id: usize,
    updates: usize,
    reads: usize,
    interval: usize,
}

impl ThreadData {
    fn new(id: usize) -> Self {
        ThreadData {
            thread_id: id,
            updates: 0,
            reads: 0,
            interval: rand::random::<usize>() % 71,
        }
    }
}

struct Data {
    element: RWLock<DataElement>,
}

impl Data {
    fn new() -> Self {
        Data {
            element: RWLock::new(DataElement {
                data: 0,
                updates: 0,
            })
        }
    }
}

struct DataElement {
    data: usize,
    updates: usize,
}

fn worker_func(data_vec: Arc<Vec<Data>>, mut thread_data: ThreadData) -> ThreadData {
    let mut index: usize = 0;
    let mut repeats: usize = 0;
    
    for iteration in 0..ITERATIONS {
        
        if iteration % thread_data.interval == 0 {
            match data_vec[index].element.write() {
                Err(e) => panic!("Error while trying to acquire write lock: {}", e),
                Ok(mut guard) => {
                    guard.data = thread_data.thread_id;
                    guard.updates += 1;
                    thread_data.updates += 1;
                },
            }
        } else {
            match data_vec[index].element.read() {
                Err(e) => panic!("Error while trying to acquire read lock: {}", e),
                Ok(guard) => {
                    thread_data.reads += 1;

                    if guard.data == thread_data.thread_id {
                        repeats += 1;
                    }
                },
            }
        }

        index += 1;
        if index >= data_vec.len() {
            index = 0;
        }
    }

    if repeats > 0 {
        println!("Thread {} found unchanged elements {} times",
                 thread_data.thread_id,
                 repeats);
    }

    return thread_data;
}

fn main() {
    let mut data_vec = Vec::with_capacity(DATASIZE);
    let mut thread_updates = 0usize;
    let mut data_updates = 0usize;

    for _ in 0..DATASIZE {
        data_vec.push(Data::new());
    }

    let data_vec = Arc::new(data_vec);
    let mut handles = Vec::with_capacity(THREADS);

    for i in 0..THREADS {
        let data_vec = data_vec.clone();
        let thread_data = ThreadData::new(i + 1);
            
        handles.push(thread::spawn(move || {
            worker_func(data_vec, thread_data)
        }));
    }

    let results: Vec<ThreadData> = handles
        .into_iter()
        .map(thread::JoinHandle::join)
        .map(Result::unwrap)
        .collect();

    // print thread specific results
    for result in results {
        thread_updates += result.updates;
        println!("{:02}: interval {}, updates {}, reads {}",
                 result.thread_id,
                 result.interval,
                 result.updates,
                 result.reads);
    }

    match Arc::try_unwrap(data_vec) {
        Err(_) => println!("Unable to exclusively access data at the end of main function."),
        Ok(data_vec) => {
            match data_vec.into_iter()
                .map(|data| data.element.into_inner())
                .collect::<Result<Vec<_>,_>>()
            {
                Err(e) => println!("Data could not be accessed. Might be still in use. {:?}", e),
                Ok(data_vec) => {
                    for i in 0..data_vec.len() {
                        let (data, updates) = (data_vec[i].data, data_vec[i].updates);
                        data_updates += updates;

                        println!("data {:02}: value {}, {:3} updates",
                                 i + 1,
                                 data,
                                 updates);
                    }
                },
            }
            
        },
    }

    println!("{} thread updates, {} data updates\n",
             thread_updates,
             data_updates);
}

