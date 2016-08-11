use std::sync::{Condvar,Mutex,Arc};
use std::collections::VecDeque;
use std::io::prelude::*;
use std::thread;

enum Request {
    Read(String, SyncType),
    Write(String),
    Quit,
}

enum SyncType {
    Sync(Arc<Response>),
    NonSync,
}

struct Response {
    ready: Condvar,
    payload: Mutex<Payload>,
}

struct Payload {
    buffer: String,
    ready_flag: bool,
}

impl Response {
    fn new() -> Self {
        Response {
            ready: Condvar::new(),
            payload: Mutex::new(Payload { buffer: String::new(), ready_flag: false }),
        }
    }
}

struct Server {
    requests: Mutex<VecDeque<Request>>,
    new_request: Condvar,
    running: Mutex<bool>,
}

impl Server {
    fn new() -> Self {
        Server {
            requests: Mutex::new(VecDeque::new()),
            new_request: Condvar::new(),
            running: Mutex::new(false),
        }
    }
}

fn server_routine(server: Arc<Server>) {
    loop {
        let request = match server.requests.lock() {
            Err(e) => panic!(format!("Server thread unable to lock requests mutex: {}", e)),
            Ok(mut requests) => {
                while requests.is_empty() {
                    requests = server.new_request.wait(requests).unwrap();
                }
                // We know there is a new request in the Deque
                requests.pop_front().unwrap()
            },
        };

        match request {
            Request::Read(prompt, SyncType::Sync(response)) => {
                let buffer = match prompt_for_input_line(prompt) {
                    Err(e) => panic!(format!("Failed to read line of input: {}", e)),
                    Ok(line) => line,
                };

                match response.payload.lock() {
                    Err(e) => panic!(format!("Unable to lock response mutex: {}", e)),
                    Ok(mut payload) => {
                        payload.buffer = buffer;
                        payload.ready_flag = true;
                        response.ready.notify_one();
                    },
                }
            },
            Request::Read(prompt, SyncType::NonSync) => {
                prompt_for_input_line(prompt).expect("Failed to read line of input");
            },
            Request::Write(buffer) => println!("{}", buffer),
            Request::Quit => break,
        }
        
    }
}

fn prompt_for_input_line(prompt: String) -> std::io::Result<String> {
    let mut buffer = String::with_capacity(128);
    print!("{}", prompt);
    try!(std::io::stdout().flush());
    try!(std::io::stdin().read_line(&mut buffer));
    Ok(buffer.trim().to_string())
}

fn server_request(server: Arc<Server>, request: Request) {
    // if server's not running, start it up.
    match server.running.lock() {
        Err(e) => panic!(format!("Failed to lock mutex to start up a server thread: {}", e)),
        Ok(mut running) => {
            if !*running {
                let server = server.clone();
                thread::spawn(move || server_routine(server));
                *running = true;
            }
        },
    }

    // make a new request to the server    
    match server.requests.lock() {
        Err(e) => panic!(format!("Failed to lock requests mutex to add a new request: {}", e)),
        Ok(mut requests) => {
            requests.push_back(request);
            server.new_request.notify_one();
        },
    }
}

fn client_routine(server: Arc<Server>, client_threads: Arc<(Mutex<usize>, Condvar)>, id: usize) {
    let prompt = format!("Client {}> ", id);

    loop {
        // issue a sychronous read request to the terminal server
        let response = Arc::new(Response::new());
        let sync_type = SyncType::Sync(response.clone());
        let read_req = Request::Read(prompt.clone(), sync_type);

        server_request(server.clone(), read_req);

        // wait for the response
        match response.payload.lock() {
            Err(e) => panic!(format!("Failed to lock mutex to wait for response: {}", e)),
            Ok(mut payload) => {
                while !payload.ready_flag {
                    payload = response.ready.wait(payload).unwrap();
                }

                // if the user entered the empty string, then this client thread is done
                if payload.buffer.is_empty() {
                    break;
                }
                
                // print the payload 4x
                for i in 0..4 {
                    let formatted = format!("({}#{}) {}", id, i, payload.buffer);
                    let write_req = Request::Write(formatted);
                    server_request(server.clone(), write_req);
                    thread::sleep(std::time::Duration::from_secs(1));
                }
            },
        };
    }

    // this client exited, so decrement client_threads and signal if this is the last client
    match client_threads.0.lock() {
        Err(e) => panic!(format!("Failed to lock client_threads mutex to decrement: {}", e)),
        Ok(mut num) => {
            *num -= 1;

            if *num == 0 {
                client_threads.1.notify_one();
            }
        }
    }
}

fn main() {
    let num_clients = 4;
    let server = Arc::new(Server::new());
    let client_threads = Arc::new((Mutex::new(num_clients), Condvar::new()));

    for i in 0..num_clients {
        let server = server.clone();
        let client_threads = client_threads.clone();
        thread::spawn(move || client_routine(server, client_threads, i));
    }

    // wait for all clients to finish
    match client_threads.0.lock() {
        Err(e) => panic!(format!("Error locking client_threads mutex to wait for them: {}", e)),
        Ok(mut num) => {
            while *num > 0 {
               num = client_threads.1.wait(num).unwrap();
            }
        }
    }

    // shut down the terminal server. we don't need the server value anymore,
    // so we might as well give ownership to server_request
    server_request(server, Request::Quit);
    
    println!("All clients are done.");    
}

