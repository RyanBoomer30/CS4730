#[macro_use]
extern crate lazy_static;

use hostname;
use std::process;
use std::net::{SocketAddr, UdpSocket};
use std::thread;
use std::sync::Mutex;

const UDP_PORT: u16 = 8888;

lazy_static! {
    static ref PEERS: Mutex<Vec<String>> = Mutex::new(Vec::new());
}

fn main() -> std::io::Result<()> {
    eprintln!("Bootstrap server started");
    
    // Check if the program is run with no arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 1 {
        eprintln!("Bootstrap server takes in no argument");
        std::process::exit(1);
    }

    let hostname = match hostname::get() {
        Ok(name) => name.into_string().unwrap_or_else(|_| "unknown".to_string()),
        Err(e) => {
            eprintln!("parse_hostfile error: Failed to get host name: {}", e);
            process::exit(1);
        }
    };

    // Throw error if hostname is not named bootstrap
    if hostname != "bootstrap" {
        eprintln!("parse_hostfile error: Hostname is not named bootstrap");
        process::exit(1);
    }

    // Start an udp listener
    let udp_socket = UdpSocket::bind(format!("0.0.0.0:{}", UDP_PORT))?;
    udp_socket.set_nonblocking(true)?;

    thread::spawn(move || {
        loop {
            let mut buf = [0; 1024];
            match udp_socket.recv_from(&mut buf) {
                Ok((size, src)) => {
                    let msg = String::from_utf8_lossy(&buf[..size]);
                    println!("Received message: '{}' from {}", msg, src);

                    // Check if the message starts with "Join:".
                    if msg.starts_with("Join:") {
                        // Extract the peer name after "Join:".
                        let parts: Vec<&str> = msg.splitn(2, ':').collect();
                        if parts.len() == 2 {
                            let peer = parts[1].trim().to_string();
                            
                            // Append the peer name to the static vector.
                            let mut peers = PEERS.lock().unwrap();
                            peers.push(peer.clone());
                            println!("Added peer '{}'. Current peers: {:?}", peer, *peers);
                        }
                    }
                }
                Err(e) => {
                    if e.kind() != std::io::ErrorKind::WouldBlock {
                        eprintln!("Error receiving UDP packet: {}", e);
                    }
                }
            }
        }
    });

    // Prevent the main thread from exiting immediately.
    loop {
        thread::park();
    }
}