use hostname;
use serde::{Deserialize, Serialize};
use serde_json;
use std::env;
use std::process;

#[derive(Serialize, Deserialize, Debug)]
struct Object {
    client_id: u64,
    object_id: u64
}

fn main() -> std::io::Result<()> {
    // Initialize the application
    let (hostname, delay_time, object_store_path) = init();

    let peername = hostname::get().unwrap_or_else(|_| {
        eprintln!("init error: Unable to get hostname");
        process::exit(1);
    });
    
    let peername = peername.to_str().unwrap_or_else(|| {
        eprintln!("init error: Unable to convert hostname to string");
        process::exit(1);
    });

    eprintln!("Peer {} joined", peername);

    // Simulate a delay before joining
    if let Some(delay) = delay_time {
        std::thread::sleep(std::time::Duration::from_secs(delay));
    }

    // Load the object store from the specified path
    let object_store: Vec<Object> = match std::fs::read_to_string(&object_store_path) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_else(|_| {
            eprintln!("init error: Unable to parse object store JSON");
            process::exit(1);
        }),
        Err(_) => {
            eprintln!("init error: Unable to read object store file");
            process::exit(1);
        }
    };

    // Print the object store
    for object in &object_store {
        println!("Object: client_id={}, object_id={}", object.client_id, object.object_id);
    }

    Ok(())
}

/// Initializes the application from command-line arguments.
// -b: The hostname of the bootstrap server.
// -d: The number of seconds to wait before joining after startup.
// -o: Path to a file containing the object store of the peer.
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

    // These two arguments are required
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