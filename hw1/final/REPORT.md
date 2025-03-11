# NOTE

At first, I was thinking of implementing Vector clock for this assignment but seeing how sending vectors over the network
and parsing them can be somewhat overkill, I decided to stick with a traditional local vector on each peers.

# The algorithm

The main loop performs two primary tasks repeatedly:

1. Send Ping Messages:

- The program sends a ping:<myName> message to all peers that are not yet marked as online (except itself).
- It resolves each peer's hostname using getaddrinfo() and sends the message using sendto().

2. Listen for Responses:

- The program listens for incoming UDP messages using recvfrom().
- If a message is received:
  - ping:<hostname>: The program responds with a pong:<myName> message to the sender.
  - pong:<hostname>: The program marks the sender (peer) as online in the online array.
- Any other message type is ignored with a debug log.

3. Check If All Peers Are Online:

- The program checks if all peers are marked as online (1 in the online array).
- When all peers are online, READY is printed.

At a higher level, the program alternates between:

- Sending pings to offline peers.
- Listening for responses.
- If no message received, then wait and repeat the loop

For future project 2 since we are iterating on the current code, it would be convenient to refactor this code so that everything
can be separated needly into stages