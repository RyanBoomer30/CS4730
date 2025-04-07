#[macro_use]
extern crate lazy_static;

use hostname;
use std::process;
use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use std::thread;
use std::sync::{Mutex, mpsc};
use std::collections::HashMap;

const TCP_PORT: u16 = 8888;

lazy_static! {
    // Global vector holding peer numbers (e.g. 1 for "n1")
    static ref PEERS: Mutex<Vec<u64>> = Mutex::new(Vec::new());
    // Global mapping from peer id to a sender for that peer’s persistent connection.
    static ref PEER_CONN: Mutex<HashMap<u64, mpsc::Sender<String>>> = Mutex::new(HashMap::new());
}

fn main() -> std::io::Result<()> {
    eprintln!("Bootstrap server started");
    
    // This bootstrap server takes no arguments.
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 1 {
        eprintln!("Bootstrap server takes in no argument");
        std::process::exit(1);
    }

    let host = match hostname::get() {
        Ok(name) => name.into_string().unwrap_or_else(|_| "unknown".to_string()),
        Err(e) => {
            eprintln!("Error: Failed to get host name: {}", e);
            process::exit(1);
        }
    };

    // Enforce that the server machine’s hostname is "bootstrap"
    if host != "bootstrap" {
        eprintln!("Error: Hostname is not named bootstrap");
        process::exit(1);
    }

    let listener = TcpListener::bind(format!("0.0.0.0:{}", TCP_PORT))
        .expect("Could not bind to address");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                // Spawn a new thread to handle each connection.
                thread::spawn(|| {
                    handle_client(stream);
                });
            },
            Err(e) => {
                println!("Error accepting connection: {}", e);
            }
        }
    }
    Ok(())
}

/// Handle a client connection.
/// Expects a message of the form "JOIN:n<number>".
fn handle_client(mut stream: TcpStream) {
    let mut buffer = [0; 512];
    // Read join message
    match stream.read(&mut buffer) {
        Ok(0) => {
            println!("Connection closed without data.");
            return;
        },
        Ok(bytes_read) => {
            let message = String::from_utf8_lossy(&buffer[..bytes_read]);
            if message.starts_with("JOIN:") {
                // Remove the "JOIN:" prefix and trim whitespace.
                let peer_str = message.trim_start_matches("JOIN:").trim();
                if let Some(num_str) = peer_str.strip_prefix('n') {
                    if let Ok(new_peer) = num_str.parse::<u64>() {
                        // Create a channel for sending messages to this peer.
                        let (tx, rx) = mpsc::channel::<String>();
                        {
                            // Insert this connection into the global mapping.
                            let mut conn_map = PEER_CONN.lock().unwrap();
                            conn_map.insert(new_peer, tx);
                        }

                        // Spawn a thread that forwards messages from rx to the client's stream.
                        let mut stream_clone = stream.try_clone().expect("Failed to clone stream");
                        thread::spawn(move || {
                            for msg in rx {
                                if let Err(e) = stream_clone.write_all(msg.as_bytes()) {
                                    println!("Error sending update to n{}: {}", new_peer, e);
                                    break;
                                }
                            }
                        });

                        // Insert the new peer into the ring and compute neighbors.
                        let (predecessor, successor, updates) = add_peer(new_peer);
                        // Build the join reply in a parsable format.
                        let predecessor_str = predecessor.map(|p| format!("n{}", p)).unwrap_or("None".to_string());
                        let successor_str = successor.map(|s| format!("n{}", s)).unwrap_or("None".to_string());
                        let reply = format!("JOIN_REPLY: predecessor={}, successor={}\n", predecessor_str, successor_str);
                        if let Err(e) = stream.write_all(reply.as_bytes()) {
                            println!("Error sending join reply to n{}: {}", new_peer, e);
                        }
                        println!("New peer n{} joined. {}", new_peer, reply.trim());
                        // Send update messages to the affected neighbor peers.
                        for (target_peer, update_msg) in updates {
                            let conn_map = PEER_CONN.lock().unwrap();
                            if let Some(sender) = conn_map.get(&target_peer) {
                                let _ = sender.send(format!("{}\n", update_msg));
                                println!("Sent update to n{}: {}", target_peer, update_msg);
                            } else {
                                println!("No connection found for n{} to send update: {}", target_peer, update_msg);
                            }
                        }
                        // Keep the connection open so that the client can receive future updates.
                        loop {
                            thread::sleep(std::time::Duration::from_secs(10));
                        }
                    } else {
                        let err_msg = "ERROR: Invalid peer number\n";
                        let _ = stream.write_all(err_msg.as_bytes());
                    }
                } else {
                    let err_msg = "ERROR: Peer name must start with 'n'\n";
                    let _ = stream.write_all(err_msg.as_bytes());
                }
            } else {
                let err_msg = "ERROR: Unknown message format\n";
                let _ = stream.write_all(err_msg.as_bytes());
            }
        },
        Err(e) => {
            println!("Error reading from stream: {}", e);
        }
    }
}

/// Inserts the new peer into the global PEERS vector and computes its neighbor relationships as a ring.
/// Also prints the full ring and returns update messages for the three affected peers (predecessor, new peer, successor).
///
/// Returns a tuple:
///   - The new peer’s predecessor (for its JOIN_REPLY)
///   - The new peer’s successor (for its JOIN_REPLY)
///   - A vector of (target_peer, update_message) pairs to notify affected peers.
fn add_peer(new_peer: u64) -> (Option<u64>, Option<u64>, Vec<(u64, String)>) {
    let mut updates = Vec::new();
    let mut peers = PEERS.lock().unwrap();
    peers.push(new_peer);
    peers.sort();

    // Print the full ring.
    let ring_string = peers.iter().map(|p| format!("n{}", p)).collect::<Vec<String>>().join(" ");
    println!("Ring: [{}]", ring_string);

    let len = peers.len();
    let idx = peers.iter().position(|&x| x == new_peer).unwrap();

    // Compute neighbor info in circular (ring) order.
    let predecessor = if idx == 0 { Some(peers[len - 1]) } else { Some(peers[idx - 1]) };
    let successor = if idx == len - 1 { Some(peers[0]) } else { Some(peers[idx + 1]) };

    // If there is only one peer, no update messages are necessary.
    if len == 1 {
        return (None, None, updates);
    }

    // For a ring, the affected peers are the new peer and its two immediate neighbors.
    // Compute new neighbor info for each affected peer.
    let get_neighbors = |peer: u64| -> (u64, u64) {
        let pos = peers.iter().position(|&x| x == peer).unwrap();
        let pred = if pos == 0 { peers[len - 1] } else { peers[pos - 1] };
        let succ = if pos == len - 1 { peers[0] } else { peers[pos + 1] };
        (pred, succ)
    };

    let affected = vec![predecessor.unwrap(), new_peer, successor.unwrap()];
    for &p in affected.iter() {
        let (pred, succ) = get_neighbors(p);
        updates.push((p, format!("Predecessor: n{}, Successor: n{}", pred, succ)));
    }
    (predecessor, successor, updates)
}
