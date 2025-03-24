use hostname;
use serde::{Deserialize, Serialize};
use serde_json;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const TCP_PORT: &str = "8889";

pub enum Role {
    Learner,
    Acceptor,
    Proposer,
}

struct UserInfo {
    name: String,
    id: u32,
}

#[derive(Serialize, Deserialize, Debug)]
struct PaxosMessage {
    peer_id: u32,
    action: String,
    message_type: String,
    message_value: String,
    proposal_num: u32,
}

/// Global Paxos state shared between proposer and acceptor.
struct PaxosState {
    promised_proposal: u32,
    accepted_proposal: Option<u32>,
    accepted_value: Option<String>,
}

fn main() {
    let (hostsfile, proposed_val, delay_time) = init();
    let (user, role, target_peers) = parse_hostfile(&hostsfile);

    // Create a shared state for Paxos that both roles will use.
    let state = Arc::new(Mutex::new(PaxosState {
        promised_proposal: 0,
        accepted_proposal: None,
        accepted_value: None,
    }));

    match role {
        Role::Proposer => {
            let message = match proposed_val {
                Some(m) => m,
                None => {
                    eprintln!("Proposer is supposed to have a proposed_val; check arguments.");
                    process::exit(1);
                }
            };

            if let Some(t) = delay_time {
                thread::sleep(Duration::from_secs(t as u64));
            }

            let proposal_num = 1; // For simplicity in Part 1, we use a fixed proposal number.
            eprintln!(
                "Proposer {} (id {}) starting Paxos with value '{}'",
                user.name, user.id, message
            );

            // Phase 1: Prepare
            let mut prepared_peers = Vec::new();
            for peer in &target_peers {
                let addr = format!("{}:{}", peer, TCP_PORT);
                match TcpStream::connect(&addr) {
                    Ok(mut stream) => {
                        println!("Proposer connected to {}", addr);
                        let prepare_msg = PaxosMessage {
                            peer_id: user.id,
                            action: "sent".to_string(),
                            message_type: "prepare".to_string(),
                            message_value: message.to_string(),
                            proposal_num,
                        };
                        let msg_json = serde_json::to_string(&prepare_msg).unwrap();
                        stream.write(msg_json.as_bytes()).unwrap();
                        println!("Proposer sent: {}", msg_json);

                        let mut buffer = [0; 512];
                        if let Ok(n) = stream.read(&mut buffer) {
                            let reply_str = String::from_utf8_lossy(&buffer[..n]);
                            println!("Proposer received: {}", reply_str);
                            let reply: PaxosMessage = serde_json::from_str(&reply_str).unwrap();
                            if reply.message_type == "prepare_ack" {
                                // Only add to the prepared_peers list if we got a prepare_ack.
                                prepared_peers.push(peer.clone());
                            }
                            // Optionally update state with reply.message_value if needed.
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to connect to {}: {}", addr, e);
                        // Optionally retry or mark this peer as not prepared.
                    }
                }
            }

            let mut prepared_peers = Vec::new();

            // Retry connecting to each peer until success (or a maximum number of retries).
            for peer in &target_peers {
                let addr = format!("{}:{}", peer, TCP_PORT);
                let mut connected = false;
                let mut retries = 0;
                while !connected && retries < 5 {
                    match TcpStream::connect(&addr) {
                        Ok(mut stream) => {
                            println!("Proposer connected to {}", addr);
                            let prepare_msg = PaxosMessage {
                                peer_id: user.id,
                                action: "sent".to_string(),
                                message_type: "prepare".to_string(),
                                message_value: message.to_string(),
                                proposal_num,
                            };
                            let msg_json = serde_json::to_string(&prepare_msg).unwrap();
                            stream.write(msg_json.as_bytes()).unwrap();
                            println!("Proposer sent: {}", msg_json);

                            let mut buffer = [0; 512];
                            if let Ok(n) = stream.read(&mut buffer) {
                                let reply_str = String::from_utf8_lossy(&buffer[..n]);
                                println!("Proposer received: {}", reply_str);
                                let reply: PaxosMessage = serde_json::from_str(&reply_str).unwrap();
                                if reply.message_type == "prepare_ack" {
                                    prepared_peers.push(peer.clone());
                                    connected = true;
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to connect to {}: {}. Retrying...", addr, e);
                            thread::sleep(Duration::from_secs(1));
                            retries += 1;
                        }
                    }
                }
                if !connected {
                    eprintln!("Unable to connect to {} after retries.", addr);
                    // Depending on your design, you might decide to exit here
                    // or to continue if a quorum is acceptable.
                }
            }

            // Phase 2: Accept - only for peers that responded in the prepare phase
            for peer in &prepared_peers {
                let addr = format!("{}:{}", peer, TCP_PORT);
                match TcpStream::connect(&addr) {
                    Ok(mut stream) => {
                        // Set a read timeout of 5 seconds.
                        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                        
                        let accept_msg = PaxosMessage {
                            peer_id: user.id,
                            action: "sent".to_string(),
                            message_type: "accept".to_string(),
                            message_value: message.to_string(),
                            proposal_num,
                        };
                        let msg_json = serde_json::to_string(&accept_msg).unwrap();
                        stream.write(msg_json.as_bytes()).unwrap();
                        println!("Proposer sent: {}", msg_json);
            
                        let mut buffer = [0; 512];
                        match stream.read(&mut buffer) {
                            Ok(n) => {
                                let reply_str = String::from_utf8_lossy(&buffer[..n]);
                                println!("Proposer received: {}", reply_str);
                                let reply: PaxosMessage = serde_json::from_str(&reply_str).unwrap();
                                if reply.message_type == "accept_ack" {
                                    let mut s = state.lock().unwrap();
                                    if s.accepted_proposal.is_none() || reply.proposal_num > s.accepted_proposal.unwrap() {
                                        s.accepted_proposal = Some(reply.proposal_num);
                                        s.accepted_value = Some(reply.message_value.clone());
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Timeout or error reading from {}: {}", addr, e);
                                // Optionally: decide to retry or mark this peer as failed.
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to connect to {}: {}", addr, e);
                    }
                }
            }

            // Print final accepted value from shared state.
            let final_state = state.lock().unwrap();
            if let Some(ref val) = final_state.accepted_value {
                eprintln!("Final accepted value: {}", val);
            } else {
                eprintln!("No value accepted.");
            }

            let chosen_msg = PaxosMessage {
                peer_id: user.id,
                action: "chose".to_string(),
                message_type: "chose".to_string(),
                message_value: message.to_string(),
                proposal_num,
            };
            eprintln!("{}", serde_json::to_string(&chosen_msg).unwrap());
        }
        Role::Acceptor => {
            let addr = format!("0.0.0.0:{}", TCP_PORT);
            let listener = TcpListener::bind(&addr).unwrap_or_else(|e| {
                eprintln!("Failed to bind to {}: {}", addr, e);
                process::exit(1);
            });
            eprintln!("Acceptor {} (id {}) listening on {}", user.name, user.id, addr);

            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        eprintln!("Accepted connection from {:?}", stream.peer_addr());
                        let state_clone = Arc::clone(&state);
                        let local_id = user.id;
                        thread::spawn(move || {
                            handle_client(stream, local_id, state_clone);
                        });
                    }
                    Err(e) => {
                        eprintln!("Error accepting connection: {}", e);
                    }
                }
            }
            let final_state = state.lock().unwrap();
            if let Some(ref val) = final_state.accepted_value {
                eprintln!("Final accepted value: {}", val);
            } else {
                eprintln!("No value accepted.");
            }
        }
        Role::Learner => {
            eprintln!("Learner {} does nothing.", user.name);
        }
    }
}

/// Initializes the application from command-line arguments.
/// Expected flags: -h <hostsfile>, -v <proposed_value>, -t <delay_time> (optional)
fn init() -> (String, Option<char>, Option<u32>) {
    let args: Vec<String> = env::args().skip(1).collect();
    let (hostsfile, proposed_val, delay_time) = args.chunks(2).fold((None, None, None), |(hf, pv, dt), pair| {
        match pair {
            [key, value] => match key.as_str() {
                "-h" => (Some(value.clone()), pv, dt),
                "-v" => (hf, value.chars().next(), dt),
                "-t" => (hf, pv, value.parse().ok()),
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
    });
    
    let hostsfile = match hostsfile {
        Some(h) => h,
        None => {
            eprintln!("init error: Missing hostsfile argument (-h)");
            process::exit(1);
        }
    };
    
    (hostsfile, proposed_val, delay_time)
}

/// Parses the hostsfile to return the current user's info, role, and target peers.
/// The UserInfo includes the name and the line number (id) where the peer appears.
fn parse_hostfile(hostsfile: &String) -> (UserInfo, Role, Vec<String>) {
    let raw_name = match hostname::get() {
        Ok(name) => name.into_string().unwrap_or_else(|_| "unknown".to_string()),
        Err(e) => {
            eprintln!("parse_hostfile error: Failed to get host name: {}", e);
            process::exit(1);
        }
    };

    let content = fs::read_to_string(hostsfile).unwrap_or_else(|err| {
        eprintln!("Error reading {}: {}", hostsfile, err);
        process::exit(1);
    });

    let mut my_roles: Vec<String> = Vec::new();
    let mut my_id: Option<u32> = None;
    let mut non_empty_line_count: u32 = 0;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        non_empty_line_count += 1;
        if let Some((peer, roles_str)) = line.split_once(':') {
            if peer.trim() == raw_name {
                my_id = Some(non_empty_line_count);
                for role in roles_str.split(',') {
                    my_roles.push(role.trim().to_string());
                }
                break;
            }
        }
    }

    let my_id = my_id.unwrap_or(0);
    let my_info = UserInfo { name: raw_name, id: my_id };

    let mut proposer_nums: Vec<String> = Vec::new();
    let mut acceptor_nums: Vec<String> = Vec::new();
    for role in &my_roles {
        if role.starts_with("proposer") {
            let num = role.trim_start_matches("proposer");
            if !num.is_empty() {
                proposer_nums.push(num.to_string());
            }
        } else if role.starts_with("acceptor") {
            let num = role.trim_start_matches("acceptor");
            if !num.is_empty() {
                acceptor_nums.push(num.to_string());
            }
        }
    }

    let mut result_peers: Vec<String> = Vec::new();
    let my_role = if !proposer_nums.is_empty() {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some((peer, roles_str)) = line.split_once(':') {
                if peer.trim() == my_info.name {
                    continue;
                }
                let roles: Vec<&str> = roles_str.split(',').map(|r| r.trim()).collect();
                for num in &proposer_nums {
                    let target_role = format!("acceptor{}", num);
                    if roles.iter().any(|&r| r == target_role) {
                        result_peers.push(peer.trim().to_string());
                        break;
                    }
                }
            }
        }
        Role::Proposer
    } else if !acceptor_nums.is_empty() {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some((peer, roles_str)) = line.split_once(':') {
                if peer.trim() == my_info.name {
                    continue;
                }
                let roles: Vec<&str> = roles_str.split(',').map(|r| r.trim()).collect();
                for num in &acceptor_nums {
                    let target_role = format!("proposer{}", num);
                    if roles.iter().any(|&r| r == target_role) {
                        result_peers.push(peer.trim().to_string());
                        break;
                    }
                }
            }
        }
        Role::Acceptor
    } else {
        result_peers = Vec::new();
        Role::Learner
    };

    result_peers.sort();
    (my_info, my_role, result_peers)
}

/// Handles an incoming TCP connection (used by both acceptors and, indirectly, by a node acting as both).
fn handle_client(mut stream: TcpStream, my_id: u32, state: Arc<Mutex<PaxosState>>) {
    let mut buffer = [0; 512];
    let n = stream.read(&mut buffer).unwrap();
    let received_str = String::from_utf8_lossy(&buffer[..n]);
    eprintln!("Received: {}", received_str);

    let msg: PaxosMessage = serde_json::from_str(&received_str).unwrap();
    let reply_type: String;
    {
        let mut s = state.lock().unwrap();
        if msg.message_type == "prepare" {
            if msg.proposal_num >= s.promised_proposal {
                s.promised_proposal = msg.proposal_num;
                reply_type = "prepare_ack".to_string();
            } else {
                reply_type = "reject_prepare".to_string();
            }
        } else if msg.message_type == "accept" {
            if msg.proposal_num >= s.promised_proposal {
                s.promised_proposal = msg.proposal_num;
                s.accepted_proposal = Some(msg.proposal_num);
                s.accepted_value = Some(msg.message_value.clone());
                reply_type = "accept_ack".to_string();
            } else {
                reply_type = "reject_accept".to_string();
            }
        } else {
            reply_type = "unknown".to_string();
        }
        if let Some(ref val) = s.accepted_value {
            eprintln!("State updated: accepted_value = {}", val);
        }
    }

    let reply_value: String = {
        let s = state.lock().unwrap();
        if let Some(ref val) = s.accepted_value {
            val.clone()
        } else if msg.message_type == "prepare" {
            msg.message_value.clone()
        } else {
            "".to_string()
        }
    };

    let reply = PaxosMessage {
        peer_id: my_id,
        action: "sent".to_string(),
        message_type: reply_type,
        message_value: reply_value,
        proposal_num: msg.proposal_num,
    };

    let reply_str = serde_json::to_string(&reply).unwrap();
    stream.write(reply_str.as_bytes()).unwrap();
    eprintln!("Sent: {}", reply_str);
}