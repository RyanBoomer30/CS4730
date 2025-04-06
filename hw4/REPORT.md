# Architecture
This program performs basic Paxos in the following order:
1. Proposer and acceptors get a list of all peers they have to connect TCP to (proposerN to acceptorN). The proposer creates TCP bind
and acceptors create TCP listener
2. The proposer saves the time of the first prepare message as the proposal_num, and send prepare to all acceptor
3. Acceptors receive prepare, saves the chosen_value temporary and minimum proposer number
4. When proposer receives all the prepare from the quorum, sends accept back to the acceptors
5. Acceptors receive accept, check if proposal_num is lower than the current proposal_num:
    - If yes, then updates the final state
    - If no, then updates the final state to be the same message_value with the lower proposal_num (This is how peer5 in part2 accepts X instead of its original proposed value Y)
6. When acceptors finished updating their state, send accept_ack back to proposer and proposer updates its final state as well when the quorum
responses are met

# Design choices
- A struct PaxosState is used to record the final state of each node
- A struct PaxosMessage is used as a message formatter
- A proposer is only in the quorum of its proposer value, and learner is not a part of any quorum
- Proposal_num is meant to be a global counter that represents the order of which messages are sent. So I use the time at which
the first prepare message is sent from the proposer to represents the proposal_num of every decision round.
- Learner are not supposed to do anything in this assignment so I'm just ignoring the nodes who are just learner entirely