use hostname;
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket, TcpListener, TcpStream};
use std::path::Path;
use std::time::Duration;
use std::thread;
use std::process;

const UDP_PORT: &str = "8888";
const TOKEN_PORT: &str = "8889";
const MAX_PEERS: usize = 1024;

fn main() {
    if let Err(e) = run() {
        eprintln!("Fatal error: {}", e);
        process::exit(1);
    }
}

fn parse_args() -> (String, usize, f64, f64, u64, bool, Option<u64>) {
    let args: Vec<String> = env::args().collect();
    let mut hostsfile: Option<String> = None;
    let mut state: usize = 0;
    let mut token_delay: f64 = 1.0;
    let mut marker_delay: f64 = 0.5;
    let mut snapshot_start: u64 = 0;
    let mut i = 1;
    let mut is_initiator = false;
    let mut snapshot_id: Option<u64> = None;

    while i < args.len() {
        match args[i].as_str() {
            "-h" => {
                if i + 1 < args.len() {
                    hostsfile = Some(args[i + 1].clone());
                    i += 1;
                } else {
                    eprintln!("Error: Missing argument for -h");
                    process::exit(1);
                }
            }
            "-x" => {
                state = 1;
                is_initiator = true;
            }
            "-t" => {
                if i + 1 < args.len() {
                    match args[i + 1].parse::<f64>() {
                        Ok(val) => token_delay = val,
                        Err(e) => {
                            eprintln!("Error: Invalid argument for -t: {}", e);
                            process::exit(1);
                        }
                    }
                    i += 1;
                } else {
                    eprintln!("Error: Missing argument for -t");
                    process::exit(1);
                }
            }
            "-m" => {
                if i + 1 < args.len() {
                    match args[i + 1].parse::<f64>() {
                        Ok(val) => marker_delay = val,
                        Err(e) => {
                            eprintln!("Error: Invalid argument for -m: {}", e);
                            process::exit(1);
                        }
                    }
                    i += 1;
                } else {
                    eprintln!("Error: Missing argument for -m");
                    process::exit(1);
                }
            }
            "-s" => {
                if i + 1 < args.len() {
                    match args[i + 1].parse::<u64>() {
                        Ok(val) => snapshot_start = val,
                        Err(e) => {
                            eprintln!("Error: Invalid argument for -s: {}", e);
                            process::exit(1);
                        }
                    }
                    i += 1;
                } else {
                    eprintln!("Error: Missing argument for -s");
                    process::exit(1);
                }
            }
            "-p" => {
                if i + 1 < args.len() {
                    match args[i + 1].parse::<u64>() {
                        Ok(val) => snapshot_id = Some(val),
                        Err(e) => {
                            eprintln!("Error: Invalid argument for -p: {}", e);
                            process::exit(1);
                        }
                    }
                    i += 1;
                } else {
                    eprintln!("Error: Missing argument for -p");
                    process::exit(1);
                }
            }
            other => {
                eprintln!("Unknown option: {}", other);
                process::exit(1);
            }
        }
        i += 1;
    }

    let hostsfile = match hostsfile {
        Some(h) => h,
        None => {
            eprintln!(
                "Error: Missing hostsfile path. Usage: {} -h <hostsfile> [-x] [-t <token_delay>] [-m <marker_delay>] [-s <snapshot_start>]",
                args[0]
            );
            process::exit(1);
        }
    };

    if !Path::new(&hostsfile).exists() {
        eprintln!("Error: Hostsfile not found: {}", hostsfile);
        process::exit(1);
    }

    (hostsfile, state, token_delay, marker_delay, snapshot_start, is_initiator, snapshot_id)
}

fn run() -> io::Result<()> {
    // Parse command-line arguments
    let (hostsfile, mut state, token_delay, marker_delay, snapshot_start, is_initiator, snapshot_id) = parse_args();

    // ========== Project 1 ========== //

    // Get our own hostname and read the list of peers from the hostsfile
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
            process::exit(1);
        }
    };

    let peer_count = peers.len();
    let predecessor = if my_id == 1 { peer_count } else { my_id - 1 };
    let successor = if my_id == peer_count { 1 } else { my_id + 1 };

    // Create and bind a UDP socket on UDP_PORT 8888.
    let socket = UdpSocket::bind(format!("0.0.0.0:{}", UDP_PORT))?;
    // Set a short read timeout (100 ms)
    socket.set_read_timeout(Some(Duration::from_millis(100)))?;

    // Hand off to the failsafe_startup loop.
    failsafe_startup(&socket, &peers, &my_name);

    // ========== Project 2 ========== //
    
    // Print our ID, state, predecessor, and successor.
    println!(
        "{{id: {}, state: {}, predecessor: {}, successor: {}}}",
        my_id, state, predecessor, successor
    );
    io::stdout().flush().unwrap();

    if marker_delay == 0.0 {
        // TEST CASE 1: Token passing in a loop once if no -m argument is provided
        token_loop(my_id, &mut state, &peers[predecessor - 1], predecessor, &peers[successor - 1], successor, token_delay, is_initiator)?;
    } else {
        // TEST CASE 2: Chandy Lamport snapshot algorithm
        token_snapshot_loop(my_id, &mut state, predecessor, successor, token_delay, marker_delay, snapshot_start, snapshot_id, &my_name, is_initiator)?;
    }
    
    return Ok(());
}

fn token_snapshot_loop(
    my_id: usize,
    state: &mut usize,
    predecessor_id: usize,
    successor_id: usize,
    token_delay: f64,
    marker_delay: f64,
    snapshot_start: u64,
    snapshot_id: Option<u64>,
    my_name: &str,
    is_initiator: bool
) -> io::Result<()> {
    // Set up TCP listener for the incoming connection from the predecessor.
    let listener_addr = format!("0.0.0.0:{}", TOKEN_PORT);
    let listener = TcpListener::bind(&listener_addr)?;

    // Spawn a thread to accept the incoming connection.
    let incoming_handle = thread::spawn(move || -> io::Result<TcpStream> {
        let (stream, _addr) = listener.accept()?;
        Ok(stream)
    });

    // Connect to our successor.
    let successor_host = format!("peer{}", successor_id);
    let successor_addr = format!("{}:{}", successor_host, TOKEN_PORT);
    let mut outgoing: Option<TcpStream> = None;
    loop {
        match TcpStream::connect(&successor_addr) {
            Ok(stream) => {
                outgoing = Some(stream);
                break;
            }
            Err(_e) => {
                thread::sleep(Duration::from_millis(500));
            }
        }
    }
    let mut outgoing = outgoing.unwrap();

    // Get the incoming connection.
    let incoming = incoming_handle.join().expect("Listener thread panicked")?;
    let mut reader = BufReader::new(incoming);

    // Do not reinitialize state here—use the mutable state passed in.
    let mut snapshot_in_progress = false;
    // snapshot_round_active remains true for the current round once triggered.
    let mut snapshot_round_active = false;
    let mut channel_closed = false; // "closes" the incoming channel for snapshot recording
    let mut channel_recording: Vec<String> = Vec::new();
    let mut token_in_hand = false; // used for logging marker info
    let mut is_snapshot_initiator = false;

    // If this process is the designated token initiator, send the initial token.
    if is_initiator {
        thread::sleep(Duration::from_secs(1));
        let token_msg = format!("token:{}", my_id);
        outgoing.write_all(token_msg.as_bytes())?;
        outgoing.write_all(b"\n")?;
        outgoing.flush()?;
        token_in_hand = true;
        println!("{{id: {}, sender: {}, receiver: {}, message:\"token\"}}", my_id, my_id, successor_id);
    }

    // Main loop: process incoming messages (either token or marker).
    loop {
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            println!("Process {}: Incoming connection closed.", my_id);
            break;
        }
        let msg = line.trim_end();

        if msg.starts_with("token:") {
            // Process token message.
            if channel_closed {
                channel_recording.push(msg.to_string());
            }
            let sender_str = msg.strip_prefix("token:").unwrap_or("?");
            println!("{{id: {}, sender: {}, receiver: {}, message:\"token\"}}", my_id, sender_str, my_id);

            token_in_hand = true;

            *state += 1;
            println!("{{id: {}, state: {}}}", my_id, *state);

            // if state equals snapshot_start and no round is active, trigger the snapshot.
            if !snapshot_in_progress && snapshot_start > 0 && *state == snapshot_start as usize && !snapshot_round_active {
                is_snapshot_initiator = true;
                snapshot_in_progress = true;

                if let Some(sid) = snapshot_id {
                    println!("{{id: {}, snapshot:\"started\", snapshot_id: {}}}", my_id, sid);
                } else {
                    println!("{{id: {}, snapshot:\"started\"}}", my_id);
                }

                channel_closed = true; // stop recording further messages on the incoming channel
                println!("{{id: {}, snapshot:\"channel closed\", channel:{}-{}, queue:{:?}}}",
                    my_id, predecessor_id, my_id, channel_recording);
                thread::sleep(Duration::from_secs_f64(marker_delay));
                let marker_msg = format!("marker:{}", my_id);
                outgoing.write_all(marker_msg.as_bytes())?;
                outgoing.write_all(b"\n")?;
                outgoing.flush()?;

                let has_token = if token_in_hand { "YES" } else { "NO" };
                if let Some(sid) = snapshot_id {
                    println!("{{id: {}, sender: {}, receiver: {}, msg:\"marker\", snapshot_id: {}, state:{}, has_token:{}}}",
                        my_id, my_id, successor_id, sid, *state, has_token);
                } else {
                    println!("{{id: {}, sender: {}, receiver: {}, msg:\"marker\", state:{}, has_token:{}}}",
                        my_id, my_id, successor_id, *state, has_token);
                }
                println!("{{id: {}, snapshot:\"complete\"}}", my_id);
                
                // Mark that this snapshot round is active.
                snapshot_round_active = true;
                // Reset temporary snapshot state for this round.
                snapshot_in_progress = false;
                channel_recording.clear();
            }

            thread::sleep(Duration::from_secs_f64(token_delay));

            // Forward the token.
            let token_msg = format!("token:{}", my_id);
            outgoing.write_all(token_msg.as_bytes())?;
            outgoing.write_all(b"\n")?;
            outgoing.flush()?;
            token_in_hand = false;
            println!("{{id: {}, sender: {}, receiver: {}, message:\"token\"}}", my_id, my_id, successor_id);

            // For the snapshot initiator, if the token round is complete, reset snapshot_round_active so that a new round can eventually be triggered.
            // This is to ensure that the snapshot is not triggered again in the same round. It's a scuff solution I'm sure there's a better way to do this.
            if is_snapshot_initiator && *state > snapshot_start as usize {
                snapshot_round_active = false;
                is_snapshot_initiator = false;
                channel_closed = false;
            }
        } else if msg.starts_with("marker:") {
            // Process marker message.
            let _marker_sender = msg.strip_prefix("marker:").unwrap_or("?");
            // For non-initiators or processes that join the snapshot,
            // if not already in snapshot and no round is active, then join.
            if !snapshot_in_progress && !snapshot_round_active {
                snapshot_in_progress = true;
                println!("{{id: {}, snapshot:\"started\"}}", my_id);
                channel_closed = true;
                println!("{{id: {}, snapshot:\"channel closed\", channel:{}-{}, queue:{:?}}}",
                    my_id, predecessor_id, my_id, channel_recording);
                thread::sleep(Duration::from_secs_f64(marker_delay));
                let marker_msg = format!("marker:{}", my_id);
                outgoing.write_all(marker_msg.as_bytes())?;
                outgoing.write_all(b"\n")?;
                outgoing.flush()?;
                let has_token = if token_in_hand { "YES" } else { "NO" };
                println!("{{id: {}, sender: {}, receiver: {}, msg:\"marker\", state:{}, has_token:{}}}",
                    my_id, my_id, successor_id, *state, has_token);
                println!("{{id: {}, snapshot:\"complete\"}}", my_id);

                // For non-initiators, we simply reset the temporary snapshot flags.
                snapshot_in_progress = false;
                channel_closed = false;
                channel_recording.clear();
            } else {
                // Already in snapshot or a round is active: if channel not yet closed, close it now.
                if !channel_closed {
                    channel_closed = true;
                    println!("{{id: {}, snapshot:\"channel closed\", channel:{}-{}, queue:{:?}}}",
                        my_id, predecessor_id, my_id, channel_recording);
                }
                println!("{{id: {}, snapshot:\"complete\"}}", my_id);
                // Reset snapshot state.
                snapshot_in_progress = false;
                channel_closed = false;
                channel_recording.clear();
            }
        } else {
            eprintln!("Process {}: Received unknown message: {}", my_id, msg);
        }
    }

    Ok(())
}

/// Send and receive tokens in a loop
fn token_loop(
    my_id: usize,
    state: &mut usize,
    predecessor_host: &str,
    predecessor_id: usize,
    successor_host: &str,
    successor_id: usize,
    token_delay: f64,
    is_initiator: bool
) -> io::Result<()> {
    // 1. Bind a TCP listener to accept a connection from our predecessor.
    let listener_addr = format!("0.0.0.0:{}", TOKEN_PORT);
    let listener = TcpListener::bind(&listener_addr)?;

    // Spawn a thread to accept the connection from our predecessor.
    let incoming_handle = thread::spawn(move || -> io::Result<TcpStream> {
        let (stream, addr) = listener.accept()?;
        Ok(stream)
    });

    // 2. Connect to our successor’s TCP listener.
    let successor_addr = format!("{}:{}", successor_host, TOKEN_PORT);
    let mut outgoing: Option<TcpStream> = None;
    loop {
        match TcpStream::connect(&successor_addr) {
            Ok(stream) => {
                outgoing = Some(stream);
                break;
            }
            Err(e) => {
                thread::sleep(Duration::from_millis(500));
            }
        }
    }
    let mut outgoing = outgoing.unwrap();

    // 3. Get the incoming connection from our predecessor.
    let incoming = incoming_handle.join().expect("Listener thread panicked")?;
    let mut reader = BufReader::new(incoming);

    // Token message format: "token:<sender_id>"
    if is_initiator {
        // Initiator: Begin the round by sending out the token.
        let token_msg = format!("token:{}", my_id);
        outgoing.write_all(token_msg.as_bytes())?;
        outgoing.write_all(b"\n")?;
        outgoing.flush()?;
        // Print token sending log.
        println!("{{id: {}, sender: {}, receiver: {}, message:\"token\"}}", my_id, my_id, successor_id);

        // Then wait to receive the token back from our predecessor.
        let mut token_line = String::new();
        reader.read_line(&mut token_line)?;
        let token_line = token_line.trim_end();
        let parts: Vec<&str> = token_line.splitn(2, ':').collect();
        if parts.len() != 2 {
            eprintln!("Process {}: Invalid token format received: '{}'", my_id, token_line);
            return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid token format"));
        }
        let sender_id: usize = parts[1].parse().unwrap_or(0);
        // Print token receipt log.
        println!("{{id: {}, sender: {}, receiver: {}, message:\"token\"}}", my_id, sender_id, my_id);
        // Process the token.
        *state += 1;
        println!("{{id: {}, state: {}}}", my_id, *state);
        thread::sleep(Duration::from_secs_f64(token_delay));
        // The token has now completed one circle; do not forward it further.
    } else {
        // Non-initiators: Wait to receive the token from our predecessor.
        let mut token_line = String::new();
        reader.read_line(&mut token_line)?;
        let token_line = token_line.trim_end();
        let parts: Vec<&str> = token_line.splitn(2, ':').collect();
        if parts.len() != 2 {
            eprintln!("Process {}: Invalid token format received: '{}'", my_id, token_line);
            return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid token format"));
        }
        let sender_id: usize = parts[1].parse().unwrap_or(0);
        // Print token receipt log.
        println!("{{id: {}, sender: {}, receiver: {}, message:\"token\"}}", my_id, sender_id, my_id);
        // Process the token.
        *state += 1;
        println!("{{id: {}, state: {}}}", my_id, *state);
        thread::sleep(Duration::from_secs_f64(token_delay));
        // Forward the token to our successor.
        let token_msg = format!("token:{}", my_id);
        outgoing.write_all(token_msg.as_bytes())?;
        outgoing.write_all(b"\n")?;
        outgoing.flush()?;
        // Print token sending log.
        println!("{{id: {}, sender: {}, receiver: {}, message:\"token\"}}", my_id, my_id, successor_id);
    }

    Ok(())
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

            let addr_str = format!("{}:{}", peer, UDP_PORT);
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

            // Wait for 2 seconds then return Ok
            thread::sleep(Duration::from_secs(2));

            return Ok(());
        }

        thread::sleep(Duration::from_millis(100)); // (100 milliseconds)
    }
}