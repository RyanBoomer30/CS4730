use std::env;
use hostname::{self};
use std::process;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket, TcpListener, TcpStream};
use std::thread;
use std::fmt;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::collections::{HashSet, HashMap};
use once_cell::sync::Lazy;

const UDP_PORT: &str = "8888";
const TCP_PORT: &str = "8889";
const HEARTBEAT_PORT: &str = "8890";
const HEARTBEAT_TIMEOUT: u64 = 3;
const LEADER_ID: u32 = 1;

// Used to store processes for removal
type RemovedSet = Arc<Mutex<HashSet<u32>>>;

// Global leader state, stored after join_start.
static LOCAL_STATE: Lazy<Mutex<Option<PeerState>>> = Lazy::new(|| Mutex::new(None));

#[derive(Clone)]
struct UserInfo {
    name: String,
    id: u32,
}

#[derive(Clone)]
struct PeerState {
    view_id: u32,
    membership: Vec<UserInfo>,
    req_counter: u32,  // Added req_counter field
}

// Display implementation for the original string representation.
impl fmt::Display for PeerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let membership_str: Vec<String> = self.membership
            .iter()
            .map(|user| format!("{}:{}", user.name, user.id))
            .collect();
        // req_counter is not printed to preserve the original format.
        write!(f, "view_id={};membership={}", self.view_id, membership_str.join(","))
    }
}

// Parse a PeerState from the original string representation.
impl FromStr for PeerState {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Check if the string contains '=' and ';' to decide which format to use.
        if s.contains("=") && s.contains(";") {
            // Old format: "view_id=42;membership=Alice:1,Bob:2,Charlie:3,David:5"
            let parts: Vec<&str> = s.split(';').collect();
            if parts.len() != 2 {
                return Err("Invalid format: expected 'view_id=...;membership=...'".to_string());
            }
            // Parse view_id.
            let view_part = parts[0].trim();
            if !view_part.starts_with("view_id=") {
                return Err("Missing 'view_id='".to_string());
            }
            let view_id_str = &view_part["view_id=".len()..];
            let view_id: u32 = view_id_str.trim().parse()
                .map_err(|e| format!("Failed to parse view_id: {}", e))?;
            // Parse membership.
            let membership_part = parts[1].trim();
            if !membership_part.starts_with("membership=") {
                return Err("Missing 'membership='".to_string());
            }
            let members_str = &membership_part["membership=".len()..];
            let mut membership = Vec::new();
            if !members_str.is_empty() {
                for entry in members_str.split(',') {
                    let entry = entry.trim();
                    if entry.is_empty() { continue; }
                    let info: Vec<&str> = entry.split(':').collect();
                    if info.len() != 2 {
                        return Err(format!("Invalid member format for entry: {}", entry));
                    }
                    let name = info[0].to_string();
                    let id: u32 = info[1].trim().parse()
                        .map_err(|e| format!("Failed to parse user id: {}", e))?;
                    membership.push(UserInfo { name, id });
                }
            }
            Ok(PeerState { view_id, membership, req_counter: 0 })
        } else {
            // New format: "<view_id>:<member1>,<member2>,..."
            let parts: Vec<&str> = s.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Err("Invalid format: expected '<view_id>:<member_list>'".to_string());
            }
            let view_id: u32 = parts[0].trim().parse()
                .map_err(|e| format!("Failed to parse view_id: {}", e))?;
            let members_str = parts[1].trim();
            let mut membership = Vec::new();
            if !members_str.is_empty() {
                for member in members_str.split(',') {
                    let member = member.trim();
                    if member.is_empty() {
                        continue;
                    }
                    let id: u32 = member.parse()
                        .map_err(|e| format!("Failed to parse member id: {}", e))?;
                    // Use a placeholder name.
                    membership.push(UserInfo { name: "unknown".to_string(), id });
                }
            }
            Ok(PeerState { view_id, membership, req_counter: 0 })
        }
    }
}

fn main() -> std::io::Result<()> {
    let (hostsfile, start_delay, join_delay, _leader_test_4) = init();
    
    if let Some(delay) = start_delay {
        eprintln!("Sleeping for {} seconds at program start...", delay);
        // eprintln!("DEBUG: main: start_delay enabled, sleeping {} seconds", delay);
        thread::sleep(Duration::from_secs(delay as u64));
    }
    
    let (name, full_list_of_peers) = parse_hostfile(&hostsfile);
    
    if has_duplicate_ids(&full_list_of_peers) {
        eprintln!("main: parse_Hostfile produced duplicated users");
        // eprintln!("DEBUG: main: duplicate user ids detected");
        process::exit(1);
    }
    
    let user_info = find_user_by_name(&full_list_of_peers, name);
    // eprintln!("DEBUG: main: Running as user '{}' with id {}", user_info.name, user_info.id);
    
    let udp_socket = UdpSocket::bind(format!("0.0.0.0:{}", UDP_PORT))?;
    udp_socket.set_read_timeout(Some(Duration::from_millis(100)))?;

    let heartbeat_socket = UdpSocket::bind(format!("0.0.0.0:{}", HEARTBEAT_PORT))?;
    
    let tcp_listener = TcpListener::bind(get_addr(&user_info.name, TCP_PORT))
        .unwrap_or_else(|_| panic!("main: Fail to bind to TCP listener"));
    // eprintln!("DEBUG: main: TCP listener bound on {}", get_addr(&user_info.name, TCP_PORT));

    // Part 2: Start sending out heartbeat detection to all the alive processes in local_state every HEARTBEAT_TIMEOUT
    // Shared structure for heartbeats: map peer id -> Instant.
    let last_hb: Arc<Mutex<HashMap<u32, Instant>>> = Arc::new(Mutex::new(HashMap::new()));
    {
        let mut map = last_hb.lock().unwrap();
        for peer in &full_list_of_peers {
            if peer.id != user_info.id {
                map.insert(peer.id, Instant::now());
            }
        }
    }
    let removed: RemovedSet = Arc::new(Mutex::new(HashSet::new()));

    // Spawn a hearbeat listener thread
    let hb_socket = heartbeat_socket.try_clone().expect("Failed to clone heartbeat socket");
    let last_hb_clone = Arc::clone(&last_hb);
    thread::spawn(move || {
        // eprintln!("DEBUG: Heartbeat listener started");
        failure_listener(hb_socket, last_hb_clone);
    });
    
    // Spawn a heartbeat sender thread: send HEARTBEAT:<local_id> to every other peer every HEARTBEAT_TIMEOUT seconds.
    let sender_socket = udp_socket.try_clone().expect("Failed to clone UDP socket for heartbeat sender");
    let peers_clone = full_list_of_peers.clone();
    thread::spawn(move || {
        loop {
            for peer in peers_clone.iter() {
                if peer.id != user_info.id {
                    let msg = format!("HEARTBEAT:{}", user_info.id);
                    send_udp_helper_port(&sender_socket, &peer.name, HEARTBEAT_PORT, &msg, "heartbeat_sender", "Failed to send heartbeat");
                }
            }
            thread::sleep(Duration::from_secs(HEARTBEAT_TIMEOUT));
        }
    });
    
    // Create local state from join_start (active membership)
    let local_state = Arc::new(Mutex::new(join_start(&udp_socket, &user_info, &full_list_of_peers, join_delay)));

    // Spawn heartbeat monitor thread.
    if user_info.id == LEADER_ID {
        let leader_state_clone = Arc::clone(&local_state);
        let removed_clone = Arc::clone(&removed);
        thread::spawn(move || {
            leader_heartbeat_monitor(last_hb, leader_state_clone, removed_clone, user_info.id);
        });
    } else {
        let last_hb_clone = Arc::clone(&last_hb);
        let local_state_clone = Arc::clone(&local_state);
        thread::spawn(move || {
            non_leader_heartbeat_monitor(last_hb_clone, local_state_clone, user_info.id);
        });
    }


    // Part 1: Spawn the TCP listener thread.
    let peers_clone = full_list_of_peers.clone();
    let listener_handle = thread::spawn(move || {
        // eprintln!("DEBUG: TCP listener thread started");
        for stream in tcp_listener.incoming() {
            if let Ok(stream) = stream {
                let mut peek_buf = [0; 5];
                let stream_clone = stream.try_clone().unwrap();
                if let Ok(n) = stream_clone.peek(&mut peek_buf) {
                    let prefix = String::from_utf8_lossy(&peek_buf[..n]);
                    // eprintln!("DEBUG: TCP listener: Received connection with prefix '{}'", prefix);
                    if prefix.starts_with("JOIN:") {
                        // eprintln!("DEBUG: TCP listener: Detected JOIN message");
                        if user_info.id == 1 {
                            // eprintln!("DEBUG: TCP listener: Acting as leader, invoking join_listener_leader");
                            join_listener_leader(stream, local_state.clone(), &peers_clone);
                        }
                    } else {
                        // eprintln!("DEBUG: TCP listener: Passing connection to join_listener_peer");
                        join_listener_peer(stream, user_info.id);
                    }
                }
            }
        }
    });
    
    // eprintln!("DEBUG: main: Blocking main thread to keep process alive");
    listener_handle.join().unwrap();
    Ok(())
}

fn get_addr(peer_name: &String, port: &str) -> String {
    format!("{}:{}", peer_name, port)
}

fn find_user_by_id(users: &Vec<UserInfo>, id: u32) -> UserInfo {
    match users.iter().find(|user| user.id == id) {
        Some(e) => {
            // eprintln!("DEBUG: find_user_by_id: Found user '{}' with id {}", e.name, e.id);
            e.clone()
        },
        None => {
            eprintln!("find_user_by_id: Can't find user with id {}", id);
            process::exit(1);
        }
    }
}

fn find_user_by_name(users: &Vec<UserInfo>, name: String) -> UserInfo {
    match users.iter().find(|user| user.name == name) {
        Some(e) => {
            // eprintln!("DEBUG: find_user_by_name: Found user '{}' with id {}", e.name, e.id);
            e.clone()
        },
        None => {
            eprintln!("find_user_by_name: Can't find user with name '{}'", name);
            process::exit(1);
        }
    }
}

fn has_duplicate_ids(users: &Vec<UserInfo>) -> bool {
    let mut seen = HashSet::new();
    for user in users {
        if !seen.insert(user.id) {
            // eprintln!("DEBUG: has_duplicate_ids: Duplicate id found: {}", user.id);
            return true;
        }
    }
    false
}

/// Init function
fn init() -> (String, Option<u32>, Option<u32>, Option<bool>) {
    let args: Vec<String> = env::args().skip(1).collect();
    
    let (hostsfile, start_delay, join_delay, leader_test_4) =
        args.chunks(2).fold(
            (None, None, None, None),
            |(hf, sd, jd, lt), pair| {
                match pair {
                    [key, value] => match key.as_str() {
                        "-h" => (Some(value.clone()), sd, jd, lt),
                        "-d" => (hf, value.parse().ok(), jd, lt),
                        "-c" => (hf, sd, value.parse().ok(), lt),
                        "-t" => (hf, sd, jd, Some(true)),
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
    
    let hostsfile = match hostsfile {
        Some(h) => h,
        None => {
            eprintln!("init error: Missing hostsfile argument (-h)");
            process::exit(1);
        }
    };
    
    // eprintln!("DEBUG: init: hostsfile = {}", hostsfile);
    (hostsfile, start_delay, join_delay, leader_test_4)
}

/// Parse hostsfile, returns current user and list of peers 
fn parse_hostfile(hostsfile: &String) -> (String, Vec<UserInfo>) {
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
                // eprintln!("DEBUG: parse_hostfile: Found user '{}' with id {}", user.name, user.id);
                peers.push(user);
            },
            Err(e) => {
                eprintln!("parse_hostfile error: Failed to read line: {}", e);
                process::exit(1);
            }
        }
    }
    
    (my_name, peers)
}

/// Protocol for when a user joins the system
fn join_start(socket: &UdpSocket, user_info: &UserInfo, full_list_of_peers: &Vec<UserInfo>, join_delay: Option<u32>) -> PeerState {
    if user_info.id == LEADER_ID {
        let mut state_opt = LOCAL_STATE.lock().unwrap();
        if let Some(ref state) = *state_opt {
            // eprintln!("DEBUG: join_start (leader): Returning existing state with view_id {}", state.view_id);
            return state.clone();
        }
        // (Spawn crash thread if join_delay is provided.)
        // eprintln!("DEBUG: join_start (leader): Leader initializing membership");
        let new_state = PeerState {
            membership: vec![user_info.clone()],
            view_id: 0,
            req_counter: 0,
        };
        *state_opt = Some(new_state.clone());

        if let Some(delay) = join_delay {
            thread::spawn(move || {
                // eprintln!("DEBUG: join_start: Peer {} will crash in {} seconds (join_delay)", user_info_clone.id, delay);
                thread::sleep(Duration::from_secs(delay as u64));
                eprintln!("join: Crashing after join_delay");
                process::exit(1);
            });
        }

        return new_state;
    } else {
        // Non-leader branch (unchanged)
        // eprintln!("DEBUG: join_start: Peer {} initiating join protocol", user_info.id);
        let leader = find_leader(&socket, &full_list_of_peers);
        // eprintln!("DEBUG: join_start: Leader found {}", leader.name);
        if leader.name == user_info.name {
            // eprintln!("DEBUG: join_start: Warning - Leader identified as self");
        }
        let join_msg = format!("JOIN:{}\n", user_info.id);
        // eprintln!("DEBUG: join_start: Sending JOIN message to leader '{}'", leader.name);
        let mut stream = TcpStream::connect(get_addr(&leader.name, TCP_PORT))
            .expect("join: Failed TCP connect");
        stream.write_all(join_msg.as_bytes())
            .expect("join: Failed to send JOIN message");
         
        if let Some(delay) = join_delay {
            thread::spawn(move || {
                // eprintln!("DEBUG: join_start: Peer {} will crash in {} seconds (join_delay)", user_info_clone.id, delay);
                thread::sleep(Duration::from_secs(delay as u64));
                eprintln!("join: Crashing after join_delay");
                process::exit(1);
            });
        }
        
        let mut reader = BufReader::new(stream);
        let mut response = String::new();
        if reader.read_line(&mut response).is_ok() {
            // eprintln!("DEBUG: join_start: Received response from leader: '{}'", response.trim());
            if response.trim().starts_with("NEWVIEW:") {
                let parts: Vec<&str> = response.trim().splitn(2, ':').collect();
                let response_peer_state: PeerState = match parts[1].parse() {
                    Ok(msg) => msg,
                    Err(e) => {
                        eprintln!("join: Fail to parse NEWVIEW: {}", e);
                        io::stdout().flush().unwrap();
                        process::exit(1);
                    }
                };
                let ids: Vec<String> = response_peer_state
                    .membership
                    .iter()
                    .map(|user| user.id.to_string())
                    .collect();
                eprintln!(
                    "{{peer_id: {}, view_id: {}, leader: {}, memb_list: [{}]}}",
                    user_info.id, response_peer_state.view_id, leader.id, ids.join(",")
                );
                return response_peer_state;
            } else {
                eprintln!("join: Leader did not respond with NEWVIEW");
                io::stdout().flush().unwrap();
                process::exit(1);
            }
        } else {
            eprintln!(
                "{{peer_id: {}, view_id: {}, leader: {}, message:\"peer {} (leader) unreachable\"}}",
                user_info.id, 0, leader.id, leader.id
            );
            io::stdout().flush().unwrap();
            process::exit(1);
        }
    }
}

/// Protocol to start a leader listener after joining
fn join_listener_leader(mut stream: TcpStream, leader_state: Arc<Mutex<PeerState>>, full_list_of_peers: &Vec<UserInfo>) {
    // eprintln!("DEBUG: join_listener_leader: Leader received connection");
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    if reader.read_line(&mut line).is_ok() {
        // eprintln!("DEBUG: join_listener_leader: Message received '{}'", line.trim());
        let trimmed = line.trim();
        if trimmed.starts_with("JOIN:") {
            let parts: Vec<&str> = trimmed.split(':').collect();
            if parts.len() == 2 {
                if let Ok(join_peer) = parts[1].parse::<u32>() {
                    // eprintln!("DEBUG: join_listener_leader: Processing JOIN from peer {}", join_peer);
                    let mut state = leader_state.lock().unwrap();
                    if state.membership.len() == 1 {
                        // eprintln!("DEBUG: join_listener_leader: Leader is alone; direct NEWVIEW will be sent");
                        let peer_info = find_user_by_id(&full_list_of_peers, join_peer);
                        state.view_id += 1;
                        state.membership.push(peer_info.clone());
                        let new_view_msg = format!(
                            "NEWVIEW:{}:{}\n",
                            state.view_id,
                            state.membership
                                .iter()
                                .map(|user| user.id.to_string())
                                .collect::<Vec<_>>()
                                .join(",")
                        );
                        // eprintln!("DEBUG: join_listener_leader: Sending NEWVIEW message on same connection: '{}'", new_view_msg.trim());
                        stream.write_all(new_view_msg.as_bytes()).expect("Failed to write NEWVIEW");
                        eprintln!(
                            "{{peer_id: 1, view_id: {}, leader: 1, memb_list: [{}]}}",
                            state.view_id,
                            state.membership
                                .iter()
                                .map(|peer| peer.id.to_string())
                                .collect::<Vec<_>>()
                                .join(",")
                        );
                    } else {
                        // eprintln!("DEBUG: join_listener_leader: Leader sending REQ messages to other peers");
                        state.req_counter += 1;
                        let req_id = state.req_counter;
                        let curr_view_id = state.view_id;
                        let mut all_ok = true;
                        for peer in state.membership.iter().filter(|p| p.id != 1) {
                            let req_msg = format!("REQ:{}:{}:ADD:{}\n", req_id, curr_view_id, join_peer);
                            // eprintln!("DEBUG: join_listener_leader: Sending REQ '{}' to peer {}", req_msg.trim(), peer.id);
                            if let Ok(mut s) = TcpStream::connect(get_addr(&peer.name, TCP_PORT)) {
                                let _ = s.write_all(req_msg.as_bytes());
                                let mut resp = String::new();
                                let mut resp_reader = BufReader::new(s);
                                if resp_reader.read_line(&mut resp).is_ok() {
                                    // eprintln!("DEBUG: join_listener_leader: Received response '{}' from peer {}", resp.trim(), peer.id);
                                    // Split the string by colon
                                    let mut parts = resp.trim().split(':');

                                    // Check if the message received starts with OK
                                    let first =  match parts.next() {
                                        Some(e) => e,
                                        None => {
                                            eprintln!("join_listener_leader: first OK message fail to parse");
                                            io::stdout().flush().unwrap();
                                            process::exit(1);
                                        }
                                    };

                                    // eprintln!("DEBUG: join_listener_leader: First part of OK: {}", first);
                                    if first != "OK" {
                                        all_ok = false;
                                    }

                                    // Check if req_id matched
                                    let second =  match parts.next() {
                                        Some(e) => e,
                                        None => {
                                            eprintln!("join_listener_leader: second OK message fail to parse");
                                            io::stdout().flush().unwrap();
                                            process::exit(1);
                                        }
                                    };

                                    // eprintln!("DEBUG: join_listener_leader: Second part of OK: {}, {}", second, &req_id.to_string());
                                    if !second.starts_with(&req_id.to_string())  {
                                        all_ok = false;
                                    }
                                } else {
                                    all_ok = false;
                                }
                            } else {
                                all_ok = false;
                            }
                        }
                        if all_ok {
                            // eprintln!("DEBUG: join_listener_leader: All REQ responses OK, updating view");
                            let peer_info = find_user_by_id(&full_list_of_peers, join_peer);
                            state.view_id += 1;
                            state.membership.push(peer_info.clone());
                            let new_view_msg = format!(
                                "NEWVIEW:{}:{}\n",
                                state.view_id,
                                state.membership
                                    .iter()
                                    .map(|user| user.id.to_string())
                                    .collect::<Vec<_>>()
                                    .join(",")
                            );
                            // eprintln!("DEBUG: join_listener_leader: Sending NEWVIEW message on same connection: '{}'", new_view_msg.trim());
                            stream.write_all(new_view_msg.as_bytes()).expect("Failed to write NEWVIEW");
                            
                            // Optionally broadcast NEWVIEW to all other members (except the joining peer and leader):
                            for peer in state.membership.iter() {
                                if peer.id != join_peer {
                                    if let Ok(mut s) = TcpStream::connect(get_addr(&peer.name, TCP_PORT)) {
                                        let _ = s.write_all(new_view_msg.as_bytes());
                                    }
                                }
                            }
                        } else {
                            // eprintln!("DEBUG: join_listener_leader: Not all peers responded OK");
                        }
                    }
                }
            }
        }
    }
}

/// Protocol to start a peer listener after joining
fn join_listener_peer(mut stream: TcpStream, local_peer_id: u32) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    if reader.read_line(&mut line).is_ok() {
        // eprintln!("DEBUG: join_listener_peer: Peer {} received message '{}'", local_peer_id, line.trim());
        let trimmed = line.trim();
        if trimmed.starts_with("REQ:") {
            let parts: Vec<&str> = trimmed.split(':').collect();
            if parts.len() >= 5 {
                let req_id = parts[1];
                let view_id = parts[2];
                let op = parts[3]; // Operation: "ADD" or "DEL"
                let target_peer = parts[4]; // The peer id to be added or deleted
                // If this is a deletion request, print the unreachable message.
                if op == "DEL" {
                    if local_peer_id != LEADER_ID { // I want to use this to avoid leader printint out twice but it still is for some reason
                        if target_peer == &LEADER_ID.to_string() {
                            eprintln!("{{peer_id: {}, view_id: {}, leader: {}, message:\"peer {} (leader) unreachable\"}}",
                                local_peer_id, view_id, LEADER_ID, target_peer);
                        } else {
                            eprintln!("{{peer_id: {}, view_id: {}, leader: {}, message:\"peer {} unreachable\"}}",
                                local_peer_id, view_id, LEADER_ID, target_peer);
                        }
                    } 
                }
                // In any case, reply with OK.
                let ok_msg = format!("OK:{}:{}\n", req_id, view_id);
                // eprintln!("DEBUG: join_listener_peer: Peer {} sending OK message '{}'", local_peer_id, ok_msg.trim());
                let _ = stream.write_all(ok_msg.as_bytes());
            }
        } else if trimmed.starts_with("NEWVIEW:") {
            let parts: Vec<&str> = trimmed.splitn(3, ':').collect();
            if parts.len() == 3 {
                let new_view_id = parts[1].parse::<u32>().unwrap_or(0);
                let memb_list_str = parts[2];
                // eprintln!("DEBUG: join_listener_peer: Peer {} updating view to {} with membership '{}'", local_peer_id, new_view_id, memb_list_str);
                // Do not modify the required output print below.
                eprintln!(
                    "{{peer_id: {}, view_id: {}, leader: 1, memb_list: [{}]}}",
                    local_peer_id, new_view_id, memb_list_str
                );
            }
        }
    }
}

//
// New helper function: send_udp_helper_port sends a UDP message to the given port.
//
fn send_udp_helper_port(socket: &UdpSocket, peer: &String, port: &str, msg: &str) {
    let addr_str = format!("{}:{}", peer, port);
    let socket_addrs: io::Result<Vec<SocketAddr>> =
        addr_str.to_socket_addrs().map(|iter| iter.collect());
    
    if let Ok(addrs) = socket_addrs {
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
            // eprintln!("DEBUG: {}:{}", function_name, function_err);
            io::stdout().flush().unwrap();
            process::exit(1);
        }
    }
}

//
// Modified failure_detection: Use HEARTBEAT_PORT instead of UDP_PORT
//
fn failure_detection(socket: &UdpSocket, peer: &String) -> bool {
    send_udp_helper_port(socket, peer, HEARTBEAT_PORT, "HEARTBEAT");
    
    let mut buffer = [0u8; 300];
    match socket.recv_from(&mut buffer) {
        Ok((received, _)) => {
            let msg = match std::str::from_utf8(&buffer[..received]) {
                Ok(m) => m,
                Err(e) => {
                    // eprintln!("DEBUG: failure_detection: Invalid UTF-8 message: {}", e);
                    return false;
                }
            };
            if msg.starts_with("ALIVE") {
                // eprintln!("DEBUG: failure_detection: Received ALIVE response");
                return true;
            }
        }
        Err(e) => {
            // eprintln!("DEBUG: failure_detection fail to read: {}", e);
        }
    }
    false
}

// Modify failure_listener to accept the shared last_hb map:
fn failure_listener(socket: UdpSocket, last_hb: Arc<Mutex<HashMap<u32, Instant>>>) {
    loop {
        let mut buffer = [0u8; 300];
        match socket.recv_from(&mut buffer) {
            Ok((received, sender_addr)) => {
                if let Ok(msg) = std::str::from_utf8(&buffer[..received]) {
                    if msg.starts_with("HEARTBEAT:") {
                        let parts: Vec<&str> = msg.trim().split(':').collect();
                        if parts.len() == 2 {
                            if let Ok(sender_id) = parts[1].parse::<u32>() {
                                let mut map = last_hb.lock().unwrap();
                                map.insert(sender_id, Instant::now());
                            }
                        }
                        let reply = "ALIVE".to_string();
                        let _ = socket.send_to(reply.as_bytes(), sender_addr);
                    }
                }
            }
            Err(e) => {
                // eprintln!("DEBUG: failure_listener: Error reading UDP: {}", e);
            }
        }
    }
}


fn find_leader(socket: &UdpSocket, peers: &Vec<UserInfo>) -> UserInfo {
    // eprintln!("DEBUG: find_leader: Starting to find a leader");

    // eprintln!("DEBUG: find_leader: Peers list:");
    for user in peers.iter() {
        // eprintln!("DEBUG: find_leader: Peer {} with id {}", user.name, user.id);
    }

    // Check if the list is already in ascending order (lowest id first)
    let is_descending = peers.windows(2).all(|w| w[1].id >= w[0].id);

    let sorted_peers = if is_descending {
        peers.clone()
    } else {
        let mut sorted = peers.clone();
        sorted.sort_by(|a, b| a.id.cmp(&b.id));
        sorted
    };

    for user in sorted_peers.iter() {
        if failure_detection(socket, &user.name) {
            // eprintln!("DEBUG: find_leader: {} passed failure_detection", user.name);
            return user.clone();
        } else {
            // eprintln!("DEBUG: find_leader: {} failed failure_detection", user.name);
            thread::sleep(Duration::from_secs(2));
        }
    }

    // eprintln!("DEBUG: find_leader: No valid leader found. Exiting...");
    io::stdout().flush().unwrap();
    process::exit(1);
}

// In the leader’s heartbeat monitor thread, check for missing heartbeats and call initiate_deletion once per crashed peer.
fn leader_heartbeat_monitor(
    last_hb: Arc<Mutex<HashMap<u32, Instant>>>,
    leader_state: Arc<Mutex<PeerState>>,
    removed: RemovedSet,
    local_id: u32,
) {
    loop {
        {
            let now = Instant::now();
            // Lock the current leader state and get the active membership IDs and current view_id.
            let state = leader_state.lock().unwrap();
            let active_ids: HashSet<u32> = state.membership.iter().map(|u| u.id).collect();
            let current_view = state.view_id;
            drop(state); // release lock
            let map = last_hb.lock().unwrap();
            for &peer_id in active_ids.iter() {
                if let Some(&timestamp) = map.get(&peer_id) {
                    if now.duration_since(timestamp) > Duration::from_secs(2 * HEARTBEAT_TIMEOUT) {
                        // Print unreachable message before initiating deletion.
                        if peer_id == LEADER_ID {
                            eprintln!(
                                "{{peer_id: {}, view_id: {}, leader: {}, message:\"peer {} (leader) unreachable\"}}",
                                local_id, current_view, LEADER_ID, peer_id
                            );
                        } else {
                            eprintln!(
                                "{{peer_id: {}, view_id: {}, leader: {}, message:\"peer {} unreachable\"}}",
                                local_id, current_view, LEADER_ID, peer_id
                            );
                        }
                        // Only call deletion if not already removed.
                        let mut rem = removed.lock().unwrap();
                        if !rem.contains(&peer_id) {
                            rem.insert(peer_id);
                            // Initiate deletion on the active membership.
                            initiate_deletion(peer_id, Arc::clone(&leader_state), &vec![]);
                        }
                    }
                }
            }
        }
        thread::sleep(Duration::from_secs(1));
    }
}

// For non-leader peers, the heartbeat monitor simply prints a message.
fn non_leader_heartbeat_monitor(last_hb: Arc<Mutex<HashMap<u32, Instant>>>, local_state: Arc<Mutex<PeerState>>, local_id: u32) {
    loop {
        {
            let now = Instant::now();
            let state = local_state.lock().unwrap();
            let active_ids: HashSet<u32> = state.membership.iter().map(|u| u.id).collect();
            drop(state);
            let map = last_hb.lock().unwrap();
            for (&peer_id, &timestamp) in map.iter() {
                if !active_ids.contains(&peer_id) { continue; }
                if now.duration_since(timestamp) > Duration::from_secs(2 * HEARTBEAT_TIMEOUT) {
                    if peer_id == LEADER_ID {
                        eprintln!("{{peer_id: {}, view_id: {}, leader: {}, message:\"peer {} (leader) unreachable\"}}",
                            local_id, 0, LEADER_ID, peer_id);
                    } else {
                        eprintln!("{{peer_id: {}, view_id: {}, leader: {}, message:\"peer {} unreachable\"}}",
                            local_id, 0, LEADER_ID, peer_id);
                    }
                }
            }
        }
        thread::sleep(Duration::from_secs(1));
    }
}

// Called by the leader when a peer is detected as crashed.
fn initiate_deletion(crashed_peer: u32, leader_state: Arc<Mutex<PeerState>>, _full_list: &Vec<UserInfo>) {
    // eprintln!("DEBUG: initiate_deletion: Initiating deletion for peer {}", crashed_peer);
    let mut state = leader_state.lock().unwrap();
    if !state.membership.iter().any(|u| u.id == crashed_peer) {
        // eprintln!("DEBUG: initiate_deletion: Peer {} not in active membership; ignoring deletion", crashed_peer);
        return;
    }
    state.req_counter += 1;
    let req_id = state.req_counter;
    let curr_view_id = state.view_id;
    let req_msg = format!("REQ:{}:{}:DEL:{}\n", req_id, curr_view_id, crashed_peer);
    // eprintln!("DEBUG: initiate_deletion: Sending deletion REQ: '{}'", req_msg.trim());
    let mut all_ok = true;
    for peer in state.membership.iter().filter(|p| p.id != LEADER_ID && p.id != crashed_peer) {
        if let Ok(mut s) = TcpStream::connect(get_addr(&peer.name, TCP_PORT)) {
            let _ = s.write_all(req_msg.as_bytes());
            let mut resp = String::new();
            let mut resp_reader = BufReader::new(s);
            if resp_reader.read_line(&mut resp).is_ok() {
                // eprintln!("DEBUG: initiate_deletion: Received response '{}' from peer {}", resp.trim(), peer.id);
                if !resp.trim().starts_with(&format!("OK:{}", req_id)) {
                    all_ok = false;
                }
            } else {
                all_ok = false;
            }
        } else {
            all_ok = false;
        }
    }
    if all_ok {
        state.view_id += 1;
        state.membership.retain(|u| u.id != crashed_peer);
        let new_view_msg = format!("NEWVIEW:{}:{}\n", state.view_id,
            state.membership.iter().map(|u| u.id.to_string()).collect::<Vec<_>>().join(","));
        // eprintln!("DEBUG: initiate_deletion: Broadcasting NEWVIEW message: '{}'", new_view_msg.trim());
        for peer in state.membership.iter() {
            if let Ok(mut s) = TcpStream::connect(get_addr(&peer.name, TCP_PORT)) {
                let _ = s.write_all(new_view_msg.as_bytes());
            }
        }
        eprintln!("{{peer_id: {}, view_id: {}, leader: {}, memb_list: [{}]}}",
            LEADER_ID,
            state.view_id,
            LEADER_ID,
            state.membership.iter().map(|u| u.id.to_string()).collect::<Vec<_>>().join(","));
    } else {
        // eprintln!("DEBUG: initiate_deletion: Not all peers responded OK; deletion aborted");
    }
}