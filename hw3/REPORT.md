# System architecture
- UDP is used for sending and receiving heartbeat messages to monitor the liveness of peers
- TCP handles reliable join requests and state updates between the leader and other peers.

The architecture is divided into two main roles:
- A leader, which is responsible for maintaining the current membership view (which is a struct
made of a view_id and a list of UserInfo structures (user name + user id)) and managing membership changes such as peer joins or deletions upon failure detection
- Non-leader peers, which follow the leader's instructions and report their heartbeat statuses.

# State diagram
Implicit state diagrams can be inferred where peers transition from an initial join state to active membership, and from an active state to a failed state when heartbeats are missed, triggering state updates across the cluster.

# Important decisions
- The LOCAL_STATE is implemented as a Lazy<Mutex<Option<PeerState>>> to avoid thread race condition. The Lazy is so this can be initiazed only when join protocol starts
- At first I only used UDP but since many messages were block, I decided to move the more important state protocols to TCP
- I tried to implement the extra credit but was sadly running into many UDP blocking and timing issues, leading to leaders re-electing themselves and miss aligning NEWVIEW updates so sadly I had to scrap the code last minute :(