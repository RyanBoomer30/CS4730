# Process Organization
The system consists of 5 processes organized in a ring. Each process:
1. Maintains a TCP connection to its successor for token passing
2. Maintains separate TCP connections to all other processes for marker messages
- I know the assignment said to keep the TCP connections for token and marker, however, this was causing token passing to be halted when a channel is closed. It should have been an easy fix with a filter but sadly I didn't have the time to implement it. Instead, has_token and state are global Arc that can be shared accross all processes and I am pretty sure the state stays consistent in between processes.
3. Has an increasing STATE counter, initialized to 0 (or 1 for the initiator)
4. Can initiate or participate in a snapshot algorithm

# Communication Infrastructure (Reason for separation in Process Organization)
**Token Channel:** A dedicated TCP connection between each process and its successor, used exclusively for token passing.
**Marker Channels:** Separate TCP connections between each pair of processes, used for distributing marker messages during the snapshot algorithm.

# State diagram
Chandy Lamport state transitions can be boiled down to:
1. [Not Recording] --- Receive First Marker ---> [Record State and Send Markers]
2. [Record State and Send Markers] --- Begin Recording on All Channels ---> [Recording on Open Channels]
3. [Recording on Open Channels] --- Receive Marker on Channel ---> [Close Channel and Stop Recording]
4. [Close Channel and Stop Recording] --- Still Have Open Channels ---> [Recording on Open Channels]
5. [Close Channel and Stop Recording] --- All Channels Closed ---> [Snapshot Complete]

Each state includes a print update except for [Recording on Open Channels] since it wasn't specified in the assignment to print out a message for listener startup 