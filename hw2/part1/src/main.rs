use hostname;
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::path::Path;
use std::time::Duration;
use std::thread;

const PORT: &str = "8888";
const MAX_PEERS: usize = 1024;

fn main() {
    if let Err(e) = run() {
        eprintln!("Fatal error: {}", e);
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    // 1. Parse command-line arguments
    let args: Vec<String> = env::args().collect();
    let mut hostsfile: Option<String> = None;
    let mut state = 0;
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "-h" => {
                if i + 1 < args.len() {
                    hostsfile = Some(args[i + 1].clone());
                    i += 1;
                } else {
                    eprintln!("Error: Missing argument for -h");
                    std::process::exit(1);
                }
            }
            "-x" => {
                state = 1;
            }
            "-t" => {
                if i + 1 < args.len() {
                    // Skip the next argument
                    i += 1;
                } else {
                    eprintln!("Error: Missing argument for -t");
                    std::process::exit(1);
                }
            }
            other => {
                eprintln!("Unknown option: {}", other);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    let hostsfile = match hostsfile {
        Some(h) => h,
        None => {
            eprintln!(
                "Error: Missing hostsfile path. Usage: {} -h <hostsfile> [-x]",
                args[0]
            );
            std::process::exit(1);
        }
    };

    if !Path::new(&hostsfile).exists() {
        eprintln!("Error: Hostsfile not found: {}", hostsfile);
        std::process::exit(1);
    }

    // 2. Get our own hostname and read the list of peers from the hostsfile
    let my_name_os = hostname::get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("gethostname failed: {}", e))
    })?;
    let my_name = my_name_os.into_string().unwrap_or_else(|_| "unknown".to_string());

    let file = File::open(&hostsfile)?;
    let reader = BufReader::new(file);
    let mut peers: Vec<String> = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        peers.push(trimmed.to_string());
        if peers.len() >= MAX_PEERS {
            break;
        }
    }

    // Determine our ID and calculate predecessor and successor.
    let mut my_id: Option<usize> = None;
    for (i, peer) in peers.iter().enumerate() {
        if peer == &my_name {
            my_id = Some(i + 1); // IDs are 1-indexed
            break;
        }
    }
    let my_id = match my_id {
        Some(id) => id,
        None => {
            eprintln!("Error: Hostname '{}' not found in the hostsfile", my_name);
            std::process::exit(1);
        }
    };

    let peer_count = peers.len();
    let predecessor = if my_id == 1 { peer_count } else { my_id - 1 };
    let successor = if my_id == peer_count { 1 } else { my_id + 1 };

    println!(
        "{{id: {}, state: {}, predecessor: {}, successor: {}}}",
        my_id, state, predecessor, successor
    );
    io::stdout().flush().unwrap();

    // 3. Create and bind a UDP socket on port 8888.
    let socket = UdpSocket::bind(format!("0.0.0.0:{}", PORT))?;
    // Set a short read timeout (100 ms)
    socket.set_read_timeout(Some(Duration::from_millis(100)))?;

    // 4. Hand off to the failsafe_startup loop.
    failsafe_startup(&socket, &peers, &my_name)
}

/// Keeps pinging until all peers are online, then prints "READY"
/// When all peers are online, run another round of pinging to check if all peers have printed "READY"
fn failsafe_startup(socket: &UdpSocket, peers: &[String], my_name: &str) -> io::Result<()> {
    let peer_count = peers.len();
    let mut online = vec![false; peer_count];
    let mut all_online_printed = false;

    loop {
        // Send "ping:<my_name>" to every peer not yet marked online, except ourselves
        for (i, peer) in peers.iter().enumerate() {
            if online[i] {
                continue;
            }
            if peer == my_name {
                online[i] = true; // mark self as online
                continue;
            }

            let addr_str = format!("{}:{}", peer, PORT);
            let socket_addrs: io::Result<Vec<SocketAddr>> =
                addr_str.to_socket_addrs().map(|iter| iter.collect());
            if let Ok(addrs) = socket_addrs {
                let msg = format!("ping:{}", my_name);
                let mut sent_ok = false;
                for addr in addrs {
                    if let Ok(sent) = socket.send_to(msg.as_bytes(), addr) {
                        if sent > 0 {
                            sent_ok = true;
                            break;
                        }
                    }
                }
                if !sent_ok {
                    println!("Failed to send ping to {}", peer);
                    io::stdout().flush().unwrap();
                }
            }
        }

        // Listen for responses
        let mut buffer = [0u8; 300];
        match socket.recv_from(&mut buffer) {
            Ok((received, sender_addr)) => {
                let msg = match std::str::from_utf8(&buffer[..received]) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Invalid UTF-8 message: {}", e);
                        continue;
                    }
                };

                if msg.starts_with("ping:") {
                    let reply = format!("pong:{}", my_name);
                    if let Err(e) = socket.send_to(reply.as_bytes(), sender_addr) {
                        eprintln!("sendto (pong) failed: {}", e);
                    }
                } else if msg.starts_with("pong:") {
                    let their_name = &msg[5..];
                    for (i, peer) in peers.iter().enumerate() {
                        if peer == their_name {
                            online[i] = true;
                        }
                    }
                } else {
                    println!("Got unknown message: {}", msg);
                    io::stdout().flush().unwrap();
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                // Runtime error from io since apparently recv_from can block each
                // other on the same socket if ran concurrently. Since it's not 
                // a big deal as we are running a loop. This is a cheat to avoid it. 
                // Source: https://users.rust-lang.org/t/udpsocket-recv-from-always-getting-resource-temporarily-unavailable-error/92451
            }
            Err(e) => {
                eprintln!("recv_from error: {}", e);
            }
        }

        // Check if all peers are online.
        if online.iter().all(|&b| b) && !all_online_printed {
            println!("READY");
            io::stdout().flush().unwrap();
            all_online_printed = true;
        }

        thread::sleep(Duration::from_millis(100)); // (100 milliseconds)
    }
}

/// Check if all peers have printed "READY"
fn ready_state(socket: &UdpSocket, peers: &[String], my_name: &str) {
    let peer_count = peers.len();
    let mut ready = vec![false; peer_count];

    loop {
        // Send "ready:<my_name>" to every peer not yet marked ready, except ourselves
        for (i, peer) in peers.iter().enumerate() {
            if ready[i] {
                continue;
            }
            if peer == my_name {
                ready[i] = true; // mark self as ready
                continue;
            }

            let addr_str = format!("{}:{}", peer, PORT);
            let socket_addrs: io::Result<Vec<SocketAddr>> =
                addr_str.to_socket_addrs().map(|iter| iter.collect());
            if let Ok(addrs) = socket_addrs {
                let msg = format!("ready:{}", my_name);
                let mut sent_ok = false;
                for addr in addrs {
                    if let Ok(sent) = socket.send_to(msg.as_bytes(), addr) {
                        if sent > 0 {
                            sent_ok = true;
                            break;
                        }
                    }
                }
                if !sent_ok {
                    println!("Failed to send ready to {}", peer);
                    io::stdout().flush().unwrap();
                }
            }
        }

        // Listen for responses
        let mut buffer = [0u8; 300];
        match socket.recv_from(&mut buffer) {
            Ok((received, sender_addr)) => {
                let msg = match std::str::from_utf8(&buffer[..received]) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Invalid UTF-8 message: {}", e);
                        continue;
                    }
                };

                if msg.starts_with("ready:") {
                    let their_name = &msg[6..];
                    for (i, peer) in peers.iter().enumerate() {
                        if peer == their_name {
                            ready[i] = true;
                        }
                    }
                } else {
                    println!("Got unknown message: {}", msg);
                    io::stdout().flush().unwrap();
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                // Runtime error from io since apparently recv_from can block each
                // other on the same socket if ran concurrently. Since it's not 
                // a big deal as we are running a loop. This is a cheat to avoid it. 
                // Source: https://users.rust-lang.org/t/udpsocket-recv-from-always-getting-resource-temporarily-unavailable-error/92451
            }
            Err(e) => {
                eprintln!("recv_from error: {}", e);
            }
        }

        // Check if all peers are ready.
        if ready.iter().all(|&b| b) {
            println!("ALL PRINTNED READY");
            io::stdout().flush().unwrap();
            break;
        }

        thread::sleep(Duration::from_millis(100)); // (100 milliseconds)
    }

    // return ready;
    // TODO
}