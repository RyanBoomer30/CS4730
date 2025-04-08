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
    // Global vector holding peer numbers
    static ref PEERS: Mutex<Vec<u64>> = Mutex::new(Vec::new());
    // Global mapping from peer id to a sender
    static ref PEER_CONN: Mutex<HashMap<u64, mpsc::Sender<String>>> = Mutex::new(HashMap::new());
}

fn main() -> std::io::Result<()> {
    // This bootstrap server takes no arguments.
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 1 {
        eprintln!("Bootstrap server takes in no argument");
        process::exit(1);
    }

    let host = match hostname::get() {
        Ok(name) => name.into_string().unwrap_or_else(|_| "unknown".to_string()),
        Err(e) => {
            eprintln!("Error: Failed to get host name: {}", e);
            process::exit(1);
        }
    };

    if host != "bootstrap" {
        eprintln!("Error: Hostname is not named bootstrap");
        process::exit(1);
    }

    let listener = TcpListener::bind(format!("0.0.0.0:{}", TCP_PORT))
        .expect("Could not bind to address");

    // Hold persistent TCP stream for peer n1
    let mut n1_stream: Option<TcpStream> = None;

    for stream in listener.incoming() {
        let stream = stream?;
        let mut peek_buf = [0u8; 64];
        let n = stream.peek(&mut peek_buf)?;
        let peek_msg = String::from_utf8_lossy(&peek_buf[..n]).to_string();

        if peek_msg.starts_with("JOIN:") {
            let peer_name = peek_msg.trim_start_matches("JOIN:").trim();
            if peer_name == "n1" {
                n1_stream = Some(stream.try_clone()?);
                let cloned_stream = stream.try_clone()?;
                thread::spawn(move || {
                    handle_client(cloned_stream, None);
                });
            } else {
                // It's a JOIN from a peer other than n1.
                thread::spawn(move || {
                    handle_client(stream, None);
                });
            }
        } else if peek_msg.starts_with("REQUEST:") {
            // For REQUEST messages, pass to n1_stream.
            if let Some(ref n1) = n1_stream {
                let n1_clone = n1.try_clone()?;
                thread::spawn(move || {
                    handle_client(stream, Some(n1_clone));
                });
            } else {
                thread::spawn(move || {
                    handle_client(stream, None);
                });
            }
        } else {
            thread::spawn(move || {
                handle_client(stream, None);
            });
        }
    }
    Ok(())
}

/// handle_client processes a connection.
/// If an optional n1_stream is provided, it is used when forwarding a REQUEST message.
fn handle_client(mut stream: TcpStream, n1_stream: Option<TcpStream>) {
    let mut buffer = [0u8; 512];
    match stream.read(&mut buffer) {
        Ok(0) => {
            println!("Connection closed without data.");
            return;
        },
        Ok(bytes_read) => {
            let message = String::from_utf8_lossy(&buffer[..bytes_read]);
            if message.starts_with("JOIN:") {
                let peer_str = message.trim_start_matches("JOIN:").trim();
                if let Some(num_str) = peer_str.strip_prefix('n') {
                    if let Ok(new_peer) = num_str.parse::<u64>() {
                        // Create a channel for sending messages to this peer.
                        let (tx, rx) = mpsc::channel::<String>();
                        {
                            let mut conn_map = PEER_CONN.lock().unwrap();
                            conn_map.insert(new_peer, tx);
                        }
                        let mut stream_clone = stream.try_clone().expect("Failed to clone stream");
                        thread::spawn(move || {
                            for msg in rx {
                                if let Err(e) = stream_clone.write_all(msg.as_bytes()) {
                                    println!("Error sending update to n{}: {}", new_peer, e);
                                    break;
                                }
                            }
                        });
                        let (predecessor, successor, updates) = add_peer(new_peer);
                        let predecessor_str = predecessor.map(|p| format!("n{}", p)).unwrap_or("None".to_string());
                        let successor_str = successor.map(|s| format!("n{}", s)).unwrap_or("None".to_string());
                        let reply = format!("JOIN_REPLY: predecessor={}, successor={}\n", predecessor_str, successor_str);
                        if let Err(e) = stream.write_all(reply.as_bytes()) {
                            println!("Error sending join reply to n{}: {}", new_peer, e);
                        }
                        for (target_peer, update_msg) in updates {
                            let conn_map = PEER_CONN.lock().unwrap();
                            if let Some(sender) = conn_map.get(&target_peer) {
                                let _ = sender.send(format!("{}\n", update_msg));
                            } else {
                                println!("No connection found for n{} to send update: {}", target_peer, update_msg);
                            }
                        }
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
            } else if message.starts_with("REQUEST:") {
                if let Some(mut n1) = n1_stream {
                    if let Err(e) = n1.write_all(message.as_bytes()) {
                        println!("Error forwarding request to n1: {}", e);
                        let _ = stream.write_all(b"ERROR: Failed to forward request to peer n1\n");
                        return;
                    }
                    n1.flush().unwrap();
                    let mut peer_buffer = [0u8; 512];
                    match n1.read(&mut peer_buffer) {
                        Ok(0) => {
                            println!("No response from n1");
                            let _ = stream.write_all(b"ERROR: No response from peer n1\n");
                        },
                        Ok(n) => {
                            let response = String::from_utf8_lossy(&peer_buffer[..n]);
                            let _ = stream.write_all(response.as_bytes());
                        },
                        Err(e) => {
                            println!("Error reading response from n1: {}", e);
                            let _ = stream.write_all(b"ERROR: Failed to read response from peer n1\n");
                        }
                    }
                } else {
                    println!("No n1 stream available for REQUEST forwarding.");
                    let _ = stream.write_all(b"ERROR: n1 not available\n");
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

/// add_peer inserts the new peer into the global PEERS vector and computes its neighbors in a ring.
fn add_peer(new_peer: u64) -> (Option<u64>, Option<u64>, Vec<(u64, String)>) {
    let mut updates = Vec::new();
    let mut peers = PEERS.lock().unwrap();
    peers.push(new_peer);
    peers.sort();

    let ring_string = peers.iter().map(|p| format!("n{}", p))
                             .collect::<Vec<String>>().join(" ");
    println!("Ring: [{}]", ring_string);

    let len = peers.len();
    let idx = peers.iter().position(|&x| x == new_peer).unwrap();
    let predecessor = if idx == 0 { Some(peers[len - 1]) } else { Some(peers[idx - 1]) };
    let successor = if idx == len - 1 { Some(peers[0]) } else { Some(peers[idx + 1]) };

    if len == 1 {
        return (None, None, updates);
    }

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
