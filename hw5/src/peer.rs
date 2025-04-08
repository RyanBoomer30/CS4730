#[macro_use]
extern crate lazy_static;

use hostname;
use serde::{Deserialize, Serialize};
use std::env;
use std::process;
use std::fs;
use std::net::{TcpStream, TcpListener};
use std::io::{Read, Write};
use std::thread;
use std::sync::{Arc, Mutex};

const TCP_PORT: u16 = 8888;
const PEER_PORT: u16 = 9999;

lazy_static! {
    static ref GLOBAL_PRED: Mutex<Option<String>> = Mutex::new(None);
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
struct Object {
    client_id: u64,
    object_id: u64,
}

struct Neighbors {
    predecessor: Option<(String, TcpStream)>,
    successor: Option<(String, TcpStream)>,
}

impl Neighbors {
    fn new() -> Self {
        Neighbors {
            predecessor: None,
            successor: None,
        }
    }
}

lazy_static! {
    static ref OBJECTS: Mutex<Vec<Object>> = Mutex::new(Vec::new());
}

fn main() -> std::io::Result<()> {
    let (bootstrap_hostname, delay_time, object_store_path) = init();

    let local_hostname = hostname::get().unwrap_or_else(|_| {
        eprintln!("main: Unable to get hostname");
        process::exit(1);
    });
    let my_str = local_hostname.to_str().unwrap_or_else(|| {
        eprintln!("main: Unable to convert hostname to string");
        process::exit(1);
    });
    let my_id: u64 = my_str.strip_prefix('n')
                          .and_then(|s| s.parse().ok())
                          .unwrap_or(0);

    let neighbors = Arc::new(Mutex::new(Neighbors::new()));
    {
        let nbrs = neighbors.clone();
        thread::spawn(move || {
            if let Err(e) = peer_listener(nbrs, my_id) {
                eprintln!("main: Error in peer listener: {}", e);
            }
        });
    }

    if let Some(delay) = delay_time {
        thread::sleep(std::time::Duration::from_secs(delay));
    }

    load_objects_from_file(&object_store_path);

    let bootstrap_addr = format!("{}:{}", bootstrap_hostname, TCP_PORT);
    let mut bs_stream = TcpStream::connect(bootstrap_addr)?;

    let join_msg = format!("JOIN:{}", my_str);
    bs_stream.write_all(join_msg.as_bytes())
             .expect("Failed to send JOIN message");

    let mut buffer = [0u8; 512];
    loop {
        match bs_stream.read(&mut buffer) {
            Ok(0) => {
                println!("Bootstrap connection closed.");
                break;
            }
            Ok(bytes_read) => {
                let response = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
                if response.starts_with("JOIN_REPLY:") {
                    if let Some((pred, succ)) = parse_join_reply(&response) {
                        if my_id == 1 {
                            *GLOBAL_PRED.lock().unwrap() = Some(pred.clone());
                        }
                        update_neighbor(&neighbors, my_id, "predecessor", &pred);
                        update_neighbor(&neighbors, my_id, "successor", &succ);
                    }
                } else if response.starts_with("UPDATE:") {
                    if let Some((direction, new_peer)) = parse_update(&response) {
                        update_neighbor(&neighbors, my_id, &direction, &new_peer);
                    }
                    
                } else if response.contains("Predecessor:") && response.contains("Successor:") {
                    if let Some((direction, new_peer)) = parse_update(&response) {
                        update_neighbor(&neighbors, my_id, &direction, &new_peer);
                    }
                    
                    if let Some((direction, new_peer)) = parse_successor(&response) {
                        update_neighbor(&neighbors, my_id, &direction, &new_peer);
                    }

                    print_neighbor_status(&neighbors);
                } else if response.starts_with("REQUEST:") {
                    let reply = handle_request(&response, neighbors.clone(), my_id);
                    bs_stream.write_all(reply.as_bytes()).unwrap();
                    bs_stream.flush().unwrap();
                }
            }
            Err(e) => {
                println!("Failed to receive data: {}", e);
                break;
            }
        }
    }
    Ok(())
}

fn load_objects_from_file(object_store_path: &str) {
    match std::fs::read_to_string(object_store_path) {
        Ok(data) => {
            let mut loaded_objects = Vec::new();
            
            for line in data.lines() {
                if let Some(obj) = parse_object_line(line) {
                    loaded_objects.push(obj);
                }
            }
            
            let mut objects = OBJECTS.lock().unwrap();
            *objects = loaded_objects;
        },
        Err(e) => {
            eprintln!("Unable to read object store file at {}: {}", object_store_path, e);
        }
    }
}

fn parse_object_line(line: &str) -> Option<Object> {
    let parts: Vec<&str> = line.trim().split("::").collect();
    if parts.len() != 2 {
        println!("Invalid object line format: {}", line);
        return None;
    }
    
    match parts[0].parse::<u64>() {
        Ok(client_id) => {
            match parts[1].parse::<u64>() {
                Ok(object_id) => {
                    Some(Object { client_id, object_id })
                },
                Err(e) => {
                    println!("Error parsing object_id in line {}: {}", line, e);
                    None
                }
            }
        },
        Err(e) => {
            println!("Error parsing client_id in line {}: {}", line, e);
            None
        }
    }
}

fn parse_successor(msg: &str) -> Option<(String, String)> {
    let tokens: Vec<&str> = msg.trim().split(',').collect();
    if tokens.len() != 2 {
        return None;
    }
    let second = tokens[1].trim();
    if second.starts_with("Successor:") {
        let succ = second.strip_prefix("Successor:")?.trim().to_string();
        return Some(("successor".to_string(), succ));
    }
    None
}

fn parse_update(msg: &str) -> Option<(String, String)> {
    let tokens: Vec<&str> = msg.trim().split(',').collect();
    if tokens.len() != 2 {
        return None;
    }
    let first = tokens[0].trim();
    let second = tokens[1].trim();
    if first.starts_with("Predecessor:") && second.starts_with("Successor:") {
        let pred = first.strip_prefix("Predecessor:")?.trim().to_string();
        return Some(("predecessor".to_string(), pred));
    }
    None
}

// Listens for peer connections and handles incoming requests.
fn peer_listener(neighbors: Arc<Mutex<Neighbors>>, my_id: u64) -> std::io::Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", PEER_PORT))?;
    
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let neighbors_clone = neighbors.clone();
                let thread_my_id = my_id;
                
                thread::spawn(move || {
                    if let Err(e) = stream.set_read_timeout(Some(std::time::Duration::from_secs(10))) {
                        println!("Peer n{}: Warning: Could not set read timeout: {}", thread_my_id, e);
                    }
                    if let Err(e) = stream.set_write_timeout(Some(std::time::Duration::from_secs(10))) {
                        println!("Peer n{}: Warning: Could not set write timeout: {}", thread_my_id, e);
                    }
                    
                    let mut buf = [0u8; 1024];
                    match stream.read(&mut buf) {
                        Ok(n) if n > 0 => {
                            let msg = String::from_utf8_lossy(&buf[..n]).to_string();
                            
                            if msg.starts_with("REQUEST:") {
                                let response = handle_request(&msg, neighbors_clone, thread_my_id);
                                
                                let mut retry_count = 0;
                                let max_retries = 3;
                                let mut success = false;
                                
                                while retry_count < max_retries && !success {
                                    match stream.write_all(response.as_bytes()) {
                                        Ok(_) => {
                                            match stream.flush() {
                                                Ok(_) => {
                                                    success = true;
                                                },
                                                Err(e) => {
                                                    println!("Peer n{}: Error flushing response (attempt {}): {}", 
                                                             thread_my_id, retry_count + 1, e);
                                                    retry_count += 1;
                                                    thread::sleep(std::time::Duration::from_millis(100));
                                                }
                                            }
                                        },
                                        Err(e) => {
                                            println!("Peer n{}: Error writing response (attempt {}): {}", 
                                                     thread_my_id, retry_count + 1, e);
                                            retry_count += 1;
                                            thread::sleep(std::time::Duration::from_millis(100));
                                        }
                                    }
                                }
                                
                                if !success {
                                    println!("Peer n{}: Failed to send response after {} attempts", 
                                             thread_my_id, max_retries);
                                }
                            } else {
                                println!("Peer n{}: Received unknown message type: {}", thread_my_id, msg.trim());
                            }
                        },
                        Ok(_) => {},
                        Err(e) => {
                            if e.kind() != std::io::ErrorKind::WouldBlock && 
                               e.kind() != std::io::ErrorKind::TimedOut {
                                println!("Peer n{}: Error reading from stream: {}", thread_my_id, e);
                            }
                        }
                    }
                });
            },
            Err(e) => {
                if e.kind() != std::io::ErrorKind::WouldBlock && 
                   e.kind() != std::io::ErrorKind::TimedOut {
                    eprintln!("Peer n{}: Error accepting connection: {}", my_id, e);
                }
            }
        }
    }
    Ok(())
}

// Handles requests using CHORD rule: if object_id ≤ my_id, handle locally; otherwise, forward to successor.
fn handle_request(request: &str, neighbors: Arc<Mutex<Neighbors>>, my_id: u64) -> String {
    let content = request.trim().strip_prefix("REQUEST:").unwrap_or("");
    let parts: Vec<&str> = content.split(',').collect();
    let mut req_id = 0;
    let mut op = "";
    let mut object_id = 0;
    let mut client_id = 0;
    
    for part in parts {
        let kv: Vec<&str> = part.split('=').collect();
        if kv.len() == 2 {
            let key = kv[0].trim();
            let value = kv[1].trim();
            match key {
                "reqID" => req_id = value.parse().unwrap_or(0),
                "op" => op = value,
                "objectID" => object_id = value.parse().unwrap_or(0),
                "clientID" => client_id = value.parse().unwrap_or(0),
                _ => {},
            }
        }
    }
    
    if object_id <= my_id {
        if op == "STORE" {
            let new_object = Object {
                client_id,
                object_id,
            };
            
            {
                let mut objects = OBJECTS.lock().unwrap();
                objects.push(new_object.clone());
            }
            
            {
                use std::fs::OpenOptions;
                match OpenOptions::new().append(true).create(true).open("Objects.txt") {
                    Ok(mut file) => {
                        use std::io::Write;
                        if let Err(e) = writeln!(file, "{}::{}", client_id, object_id) {
                            println!("Peer n{}: Error writing to Objects.txt: {}", my_id, e);
                            return format!("ERROR: Failed to store object: {}\n", e);
                        }
                    },
                    Err(e) => {
                        println!("Peer n{}: Error opening Objects.txt: {}", my_id, e);
                        return format!("ERROR: Failed to open object store: {}\n", e);
                    }
                }
            }
            
            format!("OBJ STORED: objectID={}, clientID={}, peerID=n{}\n", object_id, client_id, my_id)
        } else if op == "RETRIEVE" {
            let object_exists = {
                let objects = OBJECTS.lock().unwrap();
                objects.iter().any(|obj| obj.object_id == object_id && obj.client_id == client_id)
            };
            
            if object_exists {
                format!("OBJ RETRIEVED: objectID={}, clientID={}, peerID=n{}\n", object_id, client_id, my_id)
            } else {
                format!("OBJ NOT FOUND: objectID={}, clientID={}, peerID=n{}\n", object_id, client_id, my_id)
            }
        } else {
            println!("Peer n{}: Unknown operation: {}", my_id, op);
            "ERROR: Unknown operation\n".to_string()
        }
    } else {
        let succ;
        {
            let nbrs = neighbors.lock().unwrap();
            if let Some((s, _)) = &nbrs.successor {
                succ = s.clone();
            } else {
                return "ERROR: No successor to forward request\n".to_string();
            }
        }
        
        let peer_addr = format!("{}:{}", succ, PEER_PORT);
        
        let mut retry_count = 0;
        let max_retries = 3;
        let mut response = format!("ERROR: Failed to connect to successor {} after {} attempts\n", succ, max_retries);
        
        while retry_count < max_retries {
            match TcpStream::connect(&peer_addr) {
                Ok(mut succ_stream) => {
                    if let Err(e) = succ_stream.set_write_timeout(Some(std::time::Duration::from_secs(10))) {
                        println!("Peer n{}: Warning: Could not set write timeout: {}", my_id, e);
                    }
                    if let Err(e) = succ_stream.set_read_timeout(Some(std::time::Duration::from_secs(10))) {
                        println!("Peer n{}: Warning: Could not set read timeout: {}", my_id, e);
                    }
                    
                    match succ_stream.write_all(request.as_bytes()) {
                        Ok(_) => {
                            match succ_stream.flush() {
                                Ok(_) => {
                                    let mut buf = [0u8; 1024];
                                    match succ_stream.read(&mut buf) {
                                        Ok(n) if n > 0 => {
                                            response = String::from_utf8_lossy(&buf[..n]).to_string();
                                            break;
                                        },
                                        Ok(_) => {
                                            retry_count += 1;
                                            thread::sleep(std::time::Duration::from_millis(200));
                                        },
                                        Err(e) => {
                                            if e.kind() != std::io::ErrorKind::WouldBlock && 
                                               e.kind() != std::io::ErrorKind::TimedOut {
                                                println!("Peer n{}: Error reading from successor: {}", my_id, e);
                                            } else {
                                                println!("Peer n{}: Timed out waiting for response from successor", my_id);
                                            }
                                            response = format!("ERROR: Failed to read from successor\n");
                                            retry_count += 1;
                                            thread::sleep(std::time::Duration::from_millis(200));
                                        }
                                    }
                                },
                                Err(e) => {
                                    retry_count += 1;
                                    thread::sleep(std::time::Duration::from_millis(200));
                                }
                            }
                        },
                        Err(e) => {
                            println!("Peer n{}: Failed to write to successor: {}", my_id, e);
                            response = format!("ERROR: Failed to write to successor: {}\n", e);
                            retry_count += 1;
                            thread::sleep(std::time::Duration::from_millis(200));
                        }
                    }
                },
                Err(e) => {
                    if e.kind() != std::io::ErrorKind::TimedOut && 
                       e.kind() != std::io::ErrorKind::WouldBlock {
                        println!("Peer n{}: Could not connect to successor at {}: {}", my_id, peer_addr, e);
                    } else {
                        println!("Peer n{}: Connection to successor at {} timed out (attempt {})", 
                                 my_id, peer_addr, retry_count + 1);
                    }
                    retry_count += 1;
                    thread::sleep(std::time::Duration::from_millis(200));
                }
            }
        }
        
        response
    }
}

fn update_neighbor(neighbors: &Arc<Mutex<Neighbors>>, my_id: u64, direction: &str, new_peer: &str) {
    let mut nbrs = neighbors.lock().unwrap();
    match direction {
        "predecessor" => {
            if my_id == 1 {
                *GLOBAL_PRED.lock().unwrap() = Some(new_peer.to_string());
            }
            if new_peer == "None" {
                if nbrs.predecessor.is_some() {
                    println!("Disconnecting old predecessor connection.");
                }
                nbrs.predecessor = None;
            } else {
                nbrs.predecessor = connect_to_peer(new_peer).map(|stream| (new_peer.to_string(), stream));
            }
        },
        "successor" => {
            if new_peer == "None" {
                if nbrs.successor.is_some() {
                    println!("Disconnecting old successor connection.");
                }
                nbrs.successor = None;
            } else {
                nbrs.successor = connect_to_peer(new_peer).map(|stream| (new_peer.to_string(), stream));
            }
        },
        _ => {
            eprintln!("Unknown neighbor direction: {}", direction);
        }
    }
}

fn print_neighbor_status(neighbors: &Arc<Mutex<Neighbors>>) {
    let nbrs = neighbors.lock().unwrap();
    
    let pred_str = match &nbrs.predecessor {
        Some((peer, _)) => peer.clone(),
        None => "None".to_string()
    };
    
    let succ_str = match &nbrs.successor {
        Some((peer, _)) => peer.clone(),
        None => "None".to_string()
    };
    
    println!("Predecessor: {}, Successor: {}", pred_str, succ_str);
}

fn connect_to_peer(peer: &str) -> Option<TcpStream> {
    let addr = format!("{}:{}", peer, PEER_PORT);
    match TcpStream::connect(addr) {
        Ok(stream) => {
            Some(stream)
        },
        Err(_) => {
            None
        }
    }
}

fn parse_join_reply(reply: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = reply.trim().split(':').collect();
    if parts.len() < 2 {
        return None;
    }
    let content = parts[1].trim();
    let tokens: Vec<&str> = content.split(',').collect();
    if tokens.len() != 2 {
        return None;
    }
    let pred = tokens[0].trim().strip_prefix("predecessor=")?.trim().to_string();
    let succ = tokens[1].trim().strip_prefix("successor=")?.trim().to_string();
    Some((pred, succ))
}

fn init() -> (String, Option<u64>, String) {
    let args: Vec<String> = env::args().skip(1).collect();
    let (hostname, delay_time, object_store_path) = args.chunks(2).fold(
        (None, None, None),
        |(hn, dt, objpath), pair| {
            match pair {
                [key, value] => match key.as_str() {
                    "-b" => (Some(value.clone()), dt, objpath),
                    "-d" => (hn, value.parse().ok(), objpath),
                    "-o" => (hn, dt, Some(value.clone())),
                    other => {
                        eprintln!("init error: Unknown flag: {}", other);
                        process::exit(1);
                    }
                },
                _ => {
                    eprintln!("init error: Invalid arguments format");
                    process::exit(1);
                }
            }
        },
    );
    let hostname = hostname.unwrap_or_else(|| {
        eprintln!("init error: Missing -b flag for hostname");
        process::exit(1);
    });
    let object_store_path = object_store_path.unwrap_or_else(|| {
        eprintln!("init error: Missing -o flag for object store path");
        process::exit(1);
    });
    (hostname, delay_time, object_store_path)
}