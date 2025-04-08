use std::net::TcpStream;
use std::io::{Read, Write};
use std::env;
use std::process;
use std::thread;
use std::time::Duration;

const TCP_PORT: u16 = 8888;

fn main() -> std::io::Result<()> {
    let (bootstrap_hostname, delay_time, test_case) = init();

    if let Some(delay) = delay_time {
        thread::sleep(Duration::from_secs(delay));
    }

    // Connect to the bootstrap server.
    let bootstrap_addr = format!("{}:{}", bootstrap_hostname, TCP_PORT);
    let mut bs_stream = TcpStream::connect(&bootstrap_addr)?;

    let req_id = 1;
    let client_id = 3;

    // Depending on the test case, set the operation and object ID.
    let (op, object_id) = match test_case {
        3 => ("STORE", 9),    // Testcase 3: Store object with ID 3.
        4 => ("RETRIEVE", 10), // Testcase 4: Retrieve object with ID 3.
        5 => ("RETRIEVE", 69), // Testcase 5: Attempt to retrieve a non-existent object.
        _ => {
            eprintln!("main: Unknown test case argument");
            process::exit(1);
        }
    };

    let request_msg = format!(
        "REQUEST: reqID={}, op={}, objectID={}, clientID={}\n",
        req_id, op, object_id, client_id
    );

    // Send the request message to the bootstrap server.
    bs_stream.write_all(request_msg.as_bytes())?;
    println!("{}", request_msg.trim());

    let mut buffer = [0; 512];
    let bytes_read = bs_stream.read(&mut buffer)?;
    if bytes_read == 0 {
        println!("No response received from bootstrap server.");
        return Ok(());
    }
    let response = String::from_utf8_lossy(&buffer[..bytes_read]);
    
    // Process the response based on the test case.
    if test_case == 3 {
        // Expect a response containing "OBJ STORED".
        if response.contains("OBJ STORED") {
            println!("STORED: {}", object_id);
        } else {
            println!("Error storing object: {}", response.trim());
        }
    } else if test_case == 4 {
        // Expect a response containing "OBJ RETRIEVED".
        if response.contains("OBJ RETRIEVED") {
            println!("RETRIEVED: {}", object_id);
        } else {
            println!("Error retrieving object: {}", response.trim());
        }
    } else if test_case == 5 {
        // Expect a response containing "OBJ NOT FOUND".
        if response.contains("OBJ NOT FOUND") {
            println!("NOT FOUND: {}", object_id);
        } else {
            println!("Unexpected response: {}", response.trim());
        }
    }
    
    Ok(())
}

/// Initializes the application from command-line arguments.
///   -b : The hostname of the bootstrap server.
///   -d : (Optional) The number of seconds to wait before joining.
///   -t : Test cases (3 == STORING, 4 == RETRIEVING, 5 == RETRIEVING A NON-EXISTED ITEM)
fn init() -> (String, Option<u64>, u64) {
    let args: Vec<String> = env::args().skip(1).collect();
    let (hostname, delay_time, test_case) = args.chunks(2).fold(
        (None, None, None),
        |(hn, dt, objpath), pair| {
            match pair {
                [key, value] => match key.as_str() {
                    "-b" => (Some(value.clone()), dt, objpath),
                    "-d" => (hn, value.parse().ok(), objpath),
                    "-t" => (hn, dt, value.parse().ok()),
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

    let test_case = test_case.unwrap_or_else(|| {
        eprintln!("init error: Missing -t flag for test cases");
        process::exit(1);
    });
    (hostname, delay_time, test_case)
}
