use hostname;
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket, TcpListener, TcpStream};
use std::path::Path;
use std::time::Duration;
use std::thread;
use std::process;
use std::sync::{Arc, Mutex};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering, AtomicUsize};


const UDP_PORT: &str = "8888";
const TOKEN_PORT: u32 = 8889;

#[derive(Debug, Clone)]
struct UserInfo {
    name: String,
    id: u32,
}

fn main() {
    thread::sleep(Duration::from_secs_f64(1.0));

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
    let mut marker_delay: f64 = 0.0;
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

/// Parse hostsfile, returns current user and list of peers 
fn parse_hostfile(hostsfile: &String) -> (UserInfo, Vec<UserInfo>) {
    let my_name = match hostname::get() {
        Ok(my_name) => my_name.into_string().unwrap_or_else(|_| "unknown".to_string()),
        Err(e) => {
            eprintln!("parse_hostfile error: Failed to get host name: {}", e);
            process::exit(1);
        }
    };

    let file = File::open(&hostsfile).unwrap_or_else(|e| {
        eprintln!("parse_hostfile error: Failed to open file: {}", e);
        process::exit(1);
    });

    let reader = BufReader::new(file);
    let mut peers: Vec<UserInfo> = Vec::new();
    let mut my_user_id = 0;

    for (i, line) in reader.lines().enumerate() {
        match line {
            Ok(l) => {
                let trimmed = l.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let user = UserInfo {
                    name: trimmed.to_string(),
                    id: (i + 1) as u32,
                };
                
                if user.name == my_name {
                    my_user_id = user.id;
                }

                peers.push(user);
            }
            Err(e) => {
                eprintln!("parse_hostfile error: Failed to read line: {}", e);
                process::exit(1);
            }
        }
    }

    let my_user = UserInfo {
        name: my_name,
        id: my_user_id, // If my_name isn't found, id will be 0.
    };

    (my_user, peers)
}

// Given a user and a list of peers, return the user's predecessor
fn get_predecessor(my_user: &UserInfo, peers: &Vec<UserInfo>) -> UserInfo {
    let my_id = my_user.id;
    let peer_count = peers.len() as u32;
    let predecessor_id = if my_id == 1 { peer_count } else { my_id - 1 };
    let predecessor = peers.iter().find(|&p| p.id == predecessor_id).unwrap_or_else(|| {
        eprintln!("get_predecessor error: Predecessor not found for user '{}'", my_user.name);
        process::exit(1);
    });
    predecessor.clone()
}

// Given a user and a list of peers, return the user's successor
fn get_successor(my_user: &UserInfo, peers: &Vec<UserInfo>) -> UserInfo {
    let my_id = my_user.id;
    let peer_count = peers.len() as u32;
    let successor_id = if my_id == peer_count { 1 } else { my_id + 1 };
    let successor = peers.iter().find(|&p| p.id == successor_id).unwrap_or_else(|| {
        eprintln!("get_successor error: Successor not found for user '{}'", my_user.name);
        process::exit(1);
    });
    successor.clone()
}

fn run() -> io::Result<()> {
    // Parse command-line arguments
    let (hostsfile, mut state, token_delay, marker_delay, snapshot_start, is_initiator, snapshot_id) = parse_args();
    let (my_user, full_list_of_peers) = parse_hostfile(&hostsfile);

    // ========== Project 1 ========== //

    // Create and bind a UDP socket on UDP_PORT 8888.
    let socket = UdpSocket::bind(format!("0.0.0.0:{}", UDP_PORT))?;
    // Set a short read timeout (100 ms)
    socket.set_read_timeout(Some(Duration::from_millis(100)))?;

    // Hand off to the failsafe_startup loop.
    let peers: Vec<String> = full_list_of_peers.iter().map(|u| u.name.clone()).collect();
    let my_name = my_user.name.clone();
    let _ = failsafe_startup(&socket, &peers, &my_name);

    // ========== Project 2 ========== //
    let predecessor = get_predecessor(&my_user, &full_list_of_peers).id;
    let successor = get_successor(&my_user, &full_list_of_peers).id;

    // Print our ID, state, predecessor, and successor.
    println!(
        "{{id: {}, state: {}, predecessor: {}, successor: {}}}",
        my_user.id, state, predecessor, successor
    );
    io::stdout().flush().unwrap();

    if marker_delay == 0.0 {
        // TEST CASE 1: Token passing in a loop once if no -m argument is provided
        token_loop(my_user, full_list_of_peers, &mut state, token_delay, is_initiator)?;
    } else {
        // TEST CASE 2: Modified version of test case 1 with Chandy Lamport snapshot algorithm
        let state_arc = Arc::new(Mutex::new(state));
        token_snapshot_loop(my_user, full_list_of_peers, state_arc, token_delay, marker_delay, snapshot_start, snapshot_id, is_initiator)?;
    }
    
    return Ok(());
}

fn token_snapshot_loop(
    my_user: UserInfo,
    full_list_of_peers: Vec<UserInfo>,
    state: Arc<Mutex<usize>>,
    token_delay: f64,
    marker_delay: f64,
    snapshot_start: u64,  // seconds to wait before initiating snapshot
    snapshot_id: Option<u64>, 
    is_initiator: bool
) -> io::Result<()> {
    // 1. Bind a TCP listener for incoming connections
    let listener_addr = format!("0.0.0.0:{}", TOKEN_PORT);
    let listener = TcpListener::bind(&listener_addr)?;
    
    // 2. First, establish the TOKEN RING connection 
    // Connect to our successor in the ring
    let successor = get_successor(&my_user, &full_list_of_peers);
    let successor_addr = format!("{}:{}", successor.name, TOKEN_PORT);
    let mut outgoing: Option<TcpStream> = None;
    
    // Try to connect multiple times
    for _ in 0..10 {
        match TcpStream::connect(&successor_addr) {
            Ok(stream) => {
                outgoing = Some(stream);
                break;
            }
            Err(_) => thread::sleep(Duration::from_millis(500)),
        }
    }
    
    if outgoing.is_none() {
        return Err(io::Error::new(io::ErrorKind::ConnectionRefused, 
                                 "Could not connect to successor"));
    }
    
    let mut successor_stream = outgoing.unwrap();
    
    // Accept a connection from our predecessor
    let incoming_handle = thread::spawn(move || -> io::Result<TcpStream> {
        let (stream, _) = listener.accept()?;
        Ok(stream)
    });
    
    let predecessor_stream = incoming_handle.join().expect("Thread panicked")?;
    let mut predecessor_reader = BufReader::new(predecessor_stream.try_clone()?);
    
    // 3. Set up shared state for snapshot tracking
    let snapshot_started = Arc::new(AtomicBool::new(false));
    let snapshot_record = Arc::new(Mutex::new(Vec::<String>::new()));
    let closed_channels = Arc::new(Mutex::new(HashSet::<String>::new()));
    let closed_channels_count = Arc::new(AtomicUsize::new(0));
    let total_channels = full_list_of_peers.len() - 1; // All peers except self
    let snapshot_id_val = snapshot_id.unwrap_or(1);
    
    // 4. Add has_token flag to track token possession
    let has_token = Arc::new(AtomicBool::new(is_initiator));
    
    // 5. Set up TCP connections to all peers for markers
    let mut marker_connections: HashMap<u32, TcpStream> = HashMap::new();
    
    // Create a new listener just for marker connections (best I can do)
    let marker_listener = TcpListener::bind(format!("0.0.0.0:{}", TOKEN_PORT + 1))?;
    marker_listener.set_nonblocking(true)?;
    
    // Connect to all other peers (except self) for markers
    for peer in &full_list_of_peers {
        if peer.id != my_user.id {
            let peer_addr = format!("{}:{}", peer.name, TOKEN_PORT + 1);
            
            for attempt in 1..=5 {
                match TcpStream::connect(&peer_addr) {
                    Ok(stream) => {
                        marker_connections.insert(peer.id, stream);
                        break;
                    }
                    Err(_) if attempt < 5 => {
                        thread::sleep(Duration::from_millis(1000));
                    }
                    Err(e) => {
                        println!("Failed to establish marker connection to peer {} after 5 attempts: {}", peer.id, e);
                    }
                }
            }
        }
    }
    
    // Create a shareable version of the marker connections
    let marker_connections = Arc::new(Mutex::new(marker_connections));
    
    // 6. Start accepting marker connections from other peers (a lot of clone cause Rust borrowing cry)
    let marker_listener_clone = marker_listener.try_clone()?;
    let closed_channels_clone = Arc::clone(&closed_channels);
    let closed_channels_count_clone = Arc::clone(&closed_channels_count);
    let snapshot_started_clone = Arc::clone(&snapshot_started);
    let snapshot_record_clone = Arc::clone(&snapshot_record);
    let marker_connections_clone = Arc::clone(&marker_connections);
    let state_clone = Arc::clone(&state);
    let has_token_clone = Arc::clone(&has_token);
    let my_id = my_user.id;
    
    thread::spawn(move || {
        loop {
            match marker_listener_clone.accept() {
                Ok((stream, _)) => {
                    let closed_channels = Arc::clone(&closed_channels_clone);
                    let closed_channels_count = Arc::clone(&closed_channels_count_clone);
                    let snapshot_started = Arc::clone(&snapshot_started_clone);
                    let snapshot_record = Arc::clone(&snapshot_record_clone);
                    let marker_connections = Arc::clone(&marker_connections_clone);
                    let state = Arc::clone(&state_clone);
                    let has_token = Arc::clone(&has_token_clone);
                    let my_id = my_id;
                    
                    thread::spawn(move || {
                        let mut reader = BufReader::new(stream);
                        
                        loop {
                            let mut line = String::new();
                            match reader.read_line(&mut line) {
                                Ok(0) => break, // Connection closed
                                Ok(_) => {
                                    let line = line.trim_end();
                                    
                                    if line.starts_with("marker:") {
                                        let parts: Vec<&str> = line.splitn(3, ':').collect();
                                        if parts.len() == 3 {
                                            let marker_sender: u32 = parts[1].parse().unwrap_or(0);
                                            let marker_snapshot_id: u64 = parts[2].parse().unwrap_or(0);
                                            
                                            // Ignore marker from self
                                            if marker_sender != my_id {
                                                // Check if channel is already closed
                                                let channel_id = format!("{}-{}", marker_sender, my_id);
                                                let is_first_marker;
                                                let channel_already_closed;
                                                
                                                {
                                                    let mut closed = closed_channels.lock().unwrap();
                                                    channel_already_closed = closed.contains(&channel_id);
                                                    
                                                    if !channel_already_closed {
                                                        closed.insert(channel_id.clone());
                                                        
                                                        // Check if this is the first marker received
                                                        is_first_marker = !snapshot_started.load(Ordering::SeqCst);
                                                    } else {
                                                        is_first_marker = false;
                                                    }
                                                }
                                                
                                                if channel_already_closed {
                                                    continue;
                                                }
                                                
                                                // If this is the first marker received, start participating in the snapshot
                                                if is_first_marker {
                                                    snapshot_started.store(true, Ordering::SeqCst);
                                                    
                                                    // Current state
                                                    let current_state = *state.lock().unwrap();
                                                    
                                                    // Check if we currently have the token
                                                    let has_token_value = has_token.load(Ordering::SeqCst);
                                                    let has_token_str = if has_token_value { "YES" } else { "NO" };
                                                    
                                                    let marker_connections_clone = Arc::clone(&marker_connections);
                                                    
                                                    thread::spawn(move || {
                                                        thread::sleep(Duration::from_secs_f64(marker_delay));
                                                        
                                                        // Send markers to ALL other peers
                                                        let connections = marker_connections_clone.lock().unwrap();
                                                        for (&peer_id, stream) in connections.iter() {
                                                            if let Ok(mut stream_clone) = stream.try_clone() {
                                                                let marker_msg = format!("marker:{}:{}\n", my_id, marker_snapshot_id);
                                                                
                                                                if let Err(e) = stream_clone.write_all(marker_msg.as_bytes()) {
                                                                    eprintln!("Error sending marker to peer {}: {}", peer_id, e);
                                                                    continue;
                                                                }
                                                                
                                                                if let Err(e) = stream_clone.flush() {
                                                                    eprintln!("Error flushing marker to peer {}: {}", peer_id, e);
                                                                    continue;
                                                                }
                                                                
                                                                println!("{{proc_id:{}, snapshot_id:{}, sender:{}, receiver:{}, message:\"marker\", state:{}, has_token:\"{}\"}}",
                                                                    my_id, marker_snapshot_id, my_id, peer_id, current_state, has_token_str);
                                                            }
                                                        }
                                                    });
                                                }
                                                
                                                // Get recorded messages for this channel
                                                let tokens = {
                                                    let mut record = snapshot_record.lock().unwrap();
                                                    std::mem::take(&mut *record)
                                                };
                                                
                                                let token_list = if tokens.is_empty() {
                                                    String::new()
                                                } else {
                                                    tokens.join(", ")
                                                };
                                                
                                                println!("{{proc_id:{}, snapshot_id:{}, snapshot:\"channel closed\", channel:\"{}\", queue:[{}]}}",
                                                    my_id, marker_snapshot_id, channel_id, token_list);
                                                
                                                // Increment closed channels count
                                                closed_channels_count.fetch_add(1, Ordering::SeqCst);
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Error reading from marker connection: {}", e);
                                    break;
                                }
                            }
                        }
                    });
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    eprintln!("Error accepting marker connection: {}", e);
                    break;
                }
            }
        }
    });
    
    // 7. If this process is the token initiator, send the initial token
    if is_initiator {
        let token_msg = format!("token:{}\n", my_user.id);
        successor_stream.write_all(token_msg.as_bytes())?;
        successor_stream.flush()?;
        println!("{{id: {}, sender: {}, receiver: {}, message:\"token\"}}", 
                 my_user.id, my_user.id, successor.id);
        
        // Set has_token to false after sending
        has_token.store(false, Ordering::SeqCst);
    }
    
    // 8. Set up snapshot initiation if needed
    if snapshot_id.is_some() {
        let snapshot_id_val = snapshot_id.unwrap();
        let my_user_clone = my_user.clone();
        let marker_connections_clone = Arc::clone(&marker_connections);
        let snapshot_started_clone = Arc::clone(&snapshot_started);
        let state_clone = Arc::clone(&state);
        let has_token_clone = Arc::clone(&has_token);
        
        thread::spawn(move || {
            // Wait before starting snapshot
            thread::sleep(Duration::from_secs(snapshot_start));
            
            // Mark snapshot as started
            snapshot_started_clone.store(true, Ordering::SeqCst);
            println!("{{proc_id:{}, snapshot_id:{}, snapshot:\"started\"}}", 
                    my_user_clone.id, snapshot_id_val);
            
            thread::sleep(Duration::from_secs_f64(marker_delay));
            
            // Get current state
            let current_state = *state_clone.lock().unwrap();
            
            // Check if we currently have the token
            let has_token_value = has_token_clone.load(Ordering::SeqCst);
            let has_token_str = if has_token_value { "YES" } else { "NO" };
            
            // Send markers to ALL peers
            let connections = marker_connections_clone.lock().unwrap();
            for (&peer_id, stream) in connections.iter() {
                if let Ok(mut stream_clone) = stream.try_clone() {
                    let marker_msg = format!("marker:{}:{}\n", my_user_clone.id, snapshot_id_val);
                    
                    if let Err(e) = stream_clone.write_all(marker_msg.as_bytes()) {
                        eprintln!("Error sending marker to peer {}: {}", peer_id, e);
                        continue;
                    }
                    
                    if let Err(e) = stream_clone.flush() {
                        eprintln!("Error flushing marker to peer {}: {}", peer_id, e);
                        continue;
                    }
                    
                    println!("{{proc_id:{}, snapshot_id:{}, sender:{}, receiver:{}, message:\"marker\", state:{}, has_token:\"{}\"}}",
                        my_user_clone.id, snapshot_id_val, my_user_clone.id, peer_id, current_state, has_token_str);
                }
            }
        });
    }
    
    // 9. Set up monitoring thread
    {
        let closed_channels_count_clone = Arc::clone(&closed_channels_count);
        let total_channels_clone = total_channels;
        let my_user_clone = my_user.clone();
        let snapshot_id_val = snapshot_id_val;
        
        thread::spawn(move || {
            loop {
                let closed = closed_channels_count_clone.load(Ordering::SeqCst);
                
                if closed == total_channels_clone {
                    println!("{{proc_id:{}, snapshot_id:{}, snapshot:\"complete\"}}", 
                        my_user_clone.id, snapshot_id_val);
                    break;
                }
                thread::sleep(Duration::from_millis(500));
            }
        });
    }
    
    // 10. MAIN LOOP: Process token messages from predecessor
    loop {
        let mut line = String::new();
        match predecessor_reader.read_line(&mut line) {
            Ok(0) => break, // Connection closed
            Ok(_) => {
                let line = line.trim_end();
                
                if line.starts_with("token:") {
                    // Process token message
                    let parts: Vec<&str> = line.splitn(2, ':').collect();
                    if parts.len() != 2 {
                        eprintln!("Invalid token format: {}", line);
                        continue;
                    }
                    let sender_id: u32 = parts[1].parse().unwrap_or(0);
                    
                    println!("{{id: {}, sender: {}, receiver: {}, message:\"token\"}}", 
                        my_user.id, sender_id, my_user.id);
                    
                    // Set has_token to true when receiving token
                    has_token.store(true, Ordering::SeqCst);
                    
                    // Record token for snapshot if active
                    if snapshot_started.load(Ordering::SeqCst) {
                        let mut record = snapshot_record.lock().unwrap();
                        record.push("token".to_string());
                    }
                    
                    // Update state
                    {
                        let mut s = state.lock().unwrap();
                        *s += 1;
                        println!("{{id: {}, state: {}}}", my_user.id, *s);
                    }
                    
                    // Sleep before forwarding token
                    thread::sleep(Duration::from_secs_f64(token_delay));
                    
                    // Forward token to successor
                    println!("{{id: {}, sender: {}, receiver: {}, message:\"token\"}}", 
                        my_user.id, my_user.id, successor.id);
                    
                    let token_msg = format!("token:{}\n", my_user.id);
                    match successor_stream.write_all(token_msg.as_bytes()) {
                        Ok(_) => {
                            if let Err(e) = successor_stream.flush() {
                                eprintln!("Error flushing token to successor: {}", e);
                                break;
                            }
                            
                            // Set has_token to false after sending
                            has_token.store(false, Ordering::SeqCst);
                        }
                        Err(e) => {
                            eprintln!("Error sending token to successor: {}", e);
                            break;
                        }
                    }
                } else if line.starts_with("marker:") {
                    // Handle marker on the token channel
                    // This code ensures backward compatibility if needed
                    let parts: Vec<&str> = line.splitn(3, ':').collect();
                    if parts.len() == 3 {
                        eprintln!("Received marker on token channel, ignoring");
                    }
                } else {
                    eprintln!("Unknown message received: {}", line);
                }
            }
            Err(e) => {
                eprintln!("Error reading from predecessor: {}", e);
                break;
            }
        }
    }

    Ok(())
}

/// Send and receive tokens in a loop
fn token_loop(
    my_user: UserInfo,
    full_list_of_peers: Vec<UserInfo>,
    state: &mut usize,
    token_delay: f64,
    is_initiator: bool
) -> io::Result<()> {
    // 1. Bind a TCP listener to accept a connection from our predecessor.
    let listener_addr = format!("0.0.0.0:{}", TOKEN_PORT);
    let listener = TcpListener::bind(&listener_addr)?;

    // Spawn a thread to accept the connection from our predecessor.
    let incoming_handle = thread::spawn(move || -> io::Result<TcpStream> {
        let (stream, _) = listener.accept()?;
        Ok(stream)
    });

    // 2. Connect to our successor’s TCP listener.
    let successor = get_successor(&my_user, &full_list_of_peers);

    let successor_addr = format!("{}:{}", successor.name, TOKEN_PORT);
    let mut outgoing: Option<TcpStream> = None;
    loop {
        match TcpStream::connect(&successor_addr) {
            Ok(stream) => {
                outgoing = Some(stream);
                break;
            }
            Err(_) => {
                thread::sleep(Duration::from_millis(500));
            }
        }
    }
    let mut outgoing = outgoing.unwrap();

    // 3. Get the incoming connection from our predecessor.
    let incoming = incoming_handle.join().expect("Listener thread panicked")?;
    let mut reader = BufReader::new(incoming);

    // Token message format: "token:<sender_id>"

    // If this process is the designated token initiator, send the initial token.
    if is_initiator {
        let token_msg = format!("token:{}", my_user.id);
        outgoing.write_all(token_msg.as_bytes())?;
        outgoing.write_all(b"\n")?;
        outgoing.flush()?;
        // Print token sending log.
        println!("{{id: {}, sender: {}, receiver: {}, message:\"token\"}}", my_user.id, my_user.id, successor.id);
    }

    // Then wait to receive the token back from our predecessor.
    loop {
        let mut token_line = String::new();
        reader.read_line(&mut token_line)?;
        let token_line = token_line.trim_end();
        let parts: Vec<&str> = token_line.splitn(2, ':').collect();
        if parts.len() != 2 {
            eprintln!("Process {}: Invalid token format received: '{}'", my_user.id, token_line);
            return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid token format"));
        }
        let sender_id: usize = parts[1].parse().unwrap_or(0);
        // Print token receipt log.
        println!("{{id: {}, sender: {}, receiver: {}, message:\"token\"}}", my_user.id, sender_id, my_user.id);
        // Process the token.
        *state += 1;
        println!("{{id: {}, state: {}}}", my_user.id, *state);
        thread::sleep(Duration::from_secs_f64(token_delay));

        // Forward the token to the successor if we are not the initiator.
        if !is_initiator {
            let token_msg = format!("token:{}", my_user.id);
            outgoing.write_all(token_msg.as_bytes())?;
            outgoing.write_all(b"\n")?;
            outgoing.flush()?;
            // Print token sending log.
            println!("{{id: {}, sender: {}, receiver: {}, message:\"token\"}}", my_user.id, my_user.id, successor.id);
            break;
        } else {
            break;
        }
    }

    Ok(())
}

/// Keeps pinging until all peers are online, then prints "READY"
/// When all peers are online, run another round of pinging to check if all peers have printed "READY"
fn failsafe_startup(socket: &UdpSocket, peers: &[String], my_name: &str) -> io::Result<()> {
    let peer_count = peers.len();
    let mut online = vec![false; peer_count];

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
        if online.iter().all(|&b| b) {
            println!("READY");
            io::stdout().flush().unwrap();

            // Wait for 2 seconds then return Ok
            thread::sleep(Duration::from_secs(2));

            return Ok(());
        }

        thread::sleep(Duration::from_millis(100)); // (100 milliseconds)
    }
}