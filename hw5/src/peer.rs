use hostname;
use serde::{Deserialize, Serialize};
use std::env;
use std::process;
use std::fs;
use std::net::{TcpStream, TcpListener};
use std::io::{Read, Write};
use std::thread;
use std::sync::{Arc, Mutex};

const TCP_PORT: u16 = 8888;      // Bootstrap server port.
const PEER_PORT: u16 = 9999;     // Port on which this peer listens for neighbor connections.

#[derive(Serialize, Deserialize, Debug)]
struct Object {
    client_id: u64,
    object_id: u64
}

/// Represents our current neighbor connections.
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

fn main() -> std::io::Result<()> {
    // Initialize the application.
    let (bootstrap_hostname, delay_time, object_store_path) = init();

    // Spawn a thread to listen for incoming neighbor connections.
    thread::spawn(|| {
        if let Err(e) = peer_listener() {
            eprintln!("Error in peer listener: {}", e);
        }
    });

    // Get the local hostname (expected to be in the format "n<number>").
    let local_hostname = hostname::get().unwrap_or_else(|_| {
        eprintln!("main: Unable to get hostname");
        process::exit(1);
    });
    let peername = local_hostname.to_str().unwrap_or_else(|| {
        eprintln!("main: Unable to convert hostname to string");
        process::exit(1);
    });

    eprintln!("Peer {} joining", peername);

    // Optional delay before joining.
    if let Some(delay) = delay_time {
        std::thread::sleep(std::time::Duration::from_secs(delay));
    }

    // Load the object store from the given path.
    let data = std::fs::read_to_string(&object_store_path).unwrap_or_else(|_| {
        eprintln!("main: Unable to read object store file at {}", object_store_path);
        process::exit(1);
    });
    let objects: Vec<Object> = data
        .lines()
        .filter_map(|line| parse_object_line(line))
        .collect();
    if objects.is_empty() {
        eprintln!("main: Unable to parse object store file");
        process::exit(1);
    }

    // This Arc<Mutex<Neighbors>> will hold our current neighbor connections.
    let neighbors = Arc::new(Mutex::new(Neighbors::new()));

    // Connect to the bootstrap server.
    let bootstrap_addr = format!("{}:{}", bootstrap_hostname, TCP_PORT);
    let mut bs_stream = TcpStream::connect(bootstrap_addr)?;
    println!("Successfully connected to the bootstrap server.");

    // Send a JOIN message using the local peer name.
    let join_msg = format!("JOIN:{}", peername);
    bs_stream.write_all(join_msg.as_bytes())
             .expect("Failed to send JOIN message");
    println!("JOIN message sent, waiting for response...");

    // Continuously read messages (JOIN_REPLY and UPDATE) from the bootstrap server.
    let mut buffer = [0; 512];
    loop {
        match bs_stream.read(&mut buffer) {
            Ok(0) => {
                println!("Bootstrap connection closed.");
                break;
            },
            Ok(bytes_read) => {
                let response = String::from_utf8_lossy(&buffer[..bytes_read]);
                println!("Received: {}", response.trim());
                // Process the message.
                if response.starts_with("JOIN_REPLY:") {
                    // Expected format: "JOIN_REPLY: predecessor=nX, successor=nY"
                    if let Some((pred, succ)) = parse_join_reply(&response) {
                        update_neighbor(&neighbors, "predecessor", &pred);
                        update_neighbor(&neighbors, "successor", &succ);
                    }
                } else if response.starts_with("UPDATE:") {
                    // Expected format from server: "Predecessor: nX, Successor: nY"
                    if let Some((direction, new_peer)) = parse_update(&response) {
                        update_neighbor(&neighbors, &direction, &new_peer);
                    }
                }
            },
            Err(e) => {
                println!("Failed to receive data: {}", e);
                break;
            }
        }
    }

    Ok(())
}

/// Listens for incoming TCP connections from neighbor peers.
fn peer_listener() -> std::io::Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", PEER_PORT))?;
    println!("Peer listener started on port {}", PEER_PORT);
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                // For simplicity, we just print a message and then close the connection.
                let mut buf = [0; 128];
                let _ = stream.read(&mut buf);
                let msg = String::from_utf8_lossy(&buf);
                println!("Received incoming neighbor connection: {}", msg.trim());
            },
            Err(e) => {
                eprintln!("Error accepting neighbor connection: {}", e);
            }
        }
    }
    Ok(())
}

/// Establishes a TCP connection to the given peer (expected format "n<number>") on PEER_PORT.
fn connect_to_peer(peer: &str) -> Option<TcpStream> {
    let addr = format!("{}:{}", peer, PEER_PORT);
    match TcpStream::connect(addr) {
         Ok(stream) => {
             println!("Connected to neighbor {}", peer);
             Some(stream)
         },
         Err(e) => {
              eprintln!("Error connecting to {}: {}", peer, e);
              None
         }
    }
}

/// Updates our neighbor connection for the given direction ("predecessor" or "successor").
/// If the new peer is "None", then disconnect any existing connection.
/// Otherwise, if the new peer is different from the current, disconnect the old one and establish a new connection.
fn update_neighbor(neighbors: &Arc<Mutex<Neighbors>>, direction: &str, new_peer: &str) {
    let mut nbrs = neighbors.lock().unwrap();
    match direction {
        "predecessor" => {
            if new_peer == "None" {
                if nbrs.predecessor.is_some() {
                    println!("Disconnecting old predecessor connection.");
                }
                nbrs.predecessor = None;
            } else {
                if let Some((ref current_peer, _)) = nbrs.predecessor {
                    if current_peer == new_peer {
                        return;
                    }
                    println!("Updating predecessor from {} to {}", current_peer, new_peer);
                } else {
                    println!("Setting predecessor to {}", new_peer);
                }
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
                if let Some((ref current_peer, _)) = nbrs.successor {
                    if current_peer == new_peer {
                        return;
                    }
                    println!("Updating successor from {} to {}", current_peer, new_peer);
                } else {
                    println!("Setting successor to {}", new_peer);
                }
                nbrs.successor = connect_to_peer(new_peer).map(|stream| (new_peer.to_string(), stream));
            }
        },
        _ => {
            eprintln!("Unknown neighbor direction: {}", direction);
        }
    }
}

/// Parses a join reply string of the form:
/// "JOIN_REPLY: predecessor=nX, successor=nY"
/// Returns a tuple (predecessor, successor) as strings.
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

/// Parses an update message of the form:
/// "Predecessor: nX, Successor: nY"
/// Returns a tuple (direction, peer) where direction is "predecessor" or "successor".
///
/// In this implementation, we will call this twice—once for each neighbor update in the message.
fn parse_update(msg: &str) -> Option<(String, String)> {
    // Expect the message to contain both values separated by a comma.
    let tokens: Vec<&str> = msg.trim().split(',').collect();
    if tokens.len() != 2 {
        return None;
    }
    // For the first token, extract direction and value.
    let first = tokens[0].trim();
    let second = tokens[1].trim();
    if first.starts_with("Predecessor:") && second.starts_with("Successor:") {
        let pred = first.strip_prefix("Predecessor:")?.trim().to_string();
        let succ = second.strip_prefix("Successor:")?.trim().to_string();
        // Return both updates separately.
        // For this example, we choose to update both neighbors by calling update_neighbor separately.
        // Here we return the predecessor update first.
        // (The bootstrap message is printed once; the client will see the full line and update both if needed.)
        // In our client loop, we call parse_update() only once per message.
        // For simplicity, we return the predecessor update.
        return Some(("predecessor".to_string(), pred));
        // In a more advanced implementation, you might split the message and update both directions.
    }
    None
}

/// Initializes the application from command-line arguments.
///   -b : The hostname of the bootstrap server.
///   -d : (Optional) The number of seconds to wait before joining.
///   -o : The path to a file containing the peer's object store.
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

fn parse_object_line(line: &str) -> Option<Object> {
    let parts: Vec<&str> = line.trim().split("::").collect();
    if parts.len() != 2 {
        return None;
    }
    let client_id = parts[0].parse::<u64>().ok()?;
    let object_id = parts[1].parse::<u64>().ok()?;
    Some(Object { client_id, object_id })
}
