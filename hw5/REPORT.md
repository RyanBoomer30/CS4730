# Architecture
This program implements a basic Distributed Hash Table (DHT) system using a Chord-based ring topology with the following components:

1. Bootstrap Server (bootstrap.rs):
   - Acts as a centralized coordinator for the DHT network
   - Maintains a registry of all peers in the network
   - Handles JOIN messages from new peers
   - Coordinates peer relationships (predecessor and successor)
   - Forwards client requests to the first peer (n1)

2. Peer Node (peer.rs):
   - Joins the DHT network by connecting to the bootstrap server
   - Maintains connections with predecessor and successor peers
   - Stores objects locally based on Chord's consistent hashing rule
   - Forwards requests to successors when objects don't belong to them
   - Handles STORE and RETRIEVE operations for objects
   - Persists object data to a local file

3. Client (client.rs):
   - Connects to the bootstrap server to make requests
   - Supports STORE and RETRIEVE operations
   - Includes test cases for various scenarios (store object, retrieve object, retrieve non-existent object)

The system follows these operational steps:
1. Bootstrap server starts and listens for connections
2. Peers join the network by sending JOIN messages to bootstrap
3. Bootstrap assigns predecessor and successor to each peer
4. Clients send requests to bootstrap, which forwards them to peer n1
5. Peers route requests based on object ID using the rule: if objectID ≤ peerID, handle locally; otherwise, forward to successor
6. Objects are stored with client ID and object ID pairs
7. Client receives confirmation of successful operations or error messages

# Design choices
- Peers have knowledge only of their immediate neighbors (predecessor and successor)
- Object placement follows a simple rule: an object with ID X is stored at the first peer with ID >= X
- Bootstrap server acts as the entry point for both peers and clients
- Objects are persisted to files to survive peer restarts
- Retry mechanisms are implemented for handling network failures
- Each peer maintains a local storage of objects in memory and on disk
- Peer n1 acts as the initial contact point for all client requests