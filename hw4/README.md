# Running

The program runs exactly as indicated in the project description.

- `docker build . -t prj4` to build the image

Please contact me if nothing is printed again like one of my last project

# Errors that can get printed out
- Parsing errors from hostsfile like duplicated users or empty user
- Parsing errors when program arguments are not in the correct format
- Errors if a connection is broken mid protocol

# Log output

## Part 1: 
peer1  | {"peer_id":1,"action":"sent","message_type":"prepare","message_value":"X","proposal_num":0}
peer1  | {"peer_id":2,"action":"sent","message_type":"prepare_ack","message_value":"X","proposal_num":0}
peer2  | {"peer_id":1,"action":"sent","message_type":"prepare","message_value":"X","proposal_num":0}
peer2  | {"peer_id":2,"action":"sent","message_type":"prepare_ack","message_value":"X","proposal_num":0}
peer1  | {"peer_id":1,"action":"sent","message_type":"prepare","message_value":"X","proposal_num":0}
peer1  | {"peer_id":3,"action":"sent","message_type":"prepare_ack","message_value":"X","proposal_num":0}
peer3  | {"peer_id":1,"action":"sent","message_type":"prepare","message_value":"X","proposal_num":0}
peer3  | {"peer_id":3,"action":"sent","message_type":"prepare_ack","message_value":"X","proposal_num":0}
peer1  | {"peer_id":1,"action":"sent","message_type":"prepare","message_value":"X","proposal_num":0}
peer4  | {"peer_id":1,"action":"sent","message_type":"prepare","message_value":"X","proposal_num":0}
peer1  | {"peer_id":4,"action":"sent","message_type":"prepare_ack","message_value":"X","proposal_num":0}
peer4  | {"peer_id":4,"action":"sent","message_type":"prepare_ack","message_value":"X","proposal_num":0}
peer1  | {"peer_id":1,"action":"sent","message_type":"accept","message_value":"X","proposal_num":0}
peer1  | {"peer_id":2,"action":"sent","message_type":"accept_ack","message_value":"X","proposal_num":0}
peer1  | {"peer_id":1,"action":"sent","message_type":"accept","message_value":"X","proposal_num":0}
peer1  | {"peer_id":3,"action":"sent","message_type":"accept_ack","message_value":"X","proposal_num":0}
peer3  | {"peer_id":1,"action":"sent","message_type":"accept","message_value":"X","proposal_num":0}
peer3  | State updated: accepted_value = X
peer3  | {"peer_id":3,"action":"sent","message_type":"accept_ack","message_value":"X","proposal_num":0}
peer2  | {"peer_id":1,"action":"sent","message_type":"accept","message_value":"X","proposal_num":0}
peer2  | State updated: accepted_value = X
peer2  | {"peer_id":2,"action":"sent","message_type":"accept_ack","message_value":"X","proposal_num":0}
peer1  | {"peer_id":1,"action":"sent","message_type":"accept","message_value":"X","proposal_num":0}
peer1  | {"peer_id":4,"action":"sent","message_type":"accept_ack","message_value":"X","proposal_num":0}
peer1  | State updated: accepted_value = X
peer1  | {"peer_id":1,"action":"chose","message_type":"chose","message_value":"X","proposal_num":0}
peer4  | {"peer_id":1,"action":"sent","message_type":"accept","message_value":"X","proposal_num":0}
peer4  | State updated: accepted_value = X
peer4  | {"peer_id":4,"action":"sent","message_type":"accept_ack","message_value":"X","proposal_num":0}

## Part 2:
peer2  | {"peer_id":1,"action":"sent","message_type":"prepare","message_value":"X","proposal_num":0}
peer2  | {"peer_id":2,"action":"sent","message_type":"prepare_ack","message_value":"X","proposal_num":0}
peer1  | {"peer_id":1,"action":"sent","message_type":"prepare","message_value":"X","proposal_num":0}
peer1  | {"peer_id":2,"action":"sent","message_type":"prepare_ack","message_value":"X","proposal_num":0}
peer1  | {"peer_id":1,"action":"sent","message_type":"prepare","message_value":"X","proposal_num":0}
peer3  | {"peer_id":1,"action":"sent","message_type":"prepare","message_value":"X","proposal_num":0}
peer3  | {"peer_id":3,"action":"sent","message_type":"prepare_ack","message_value":"X","proposal_num":0}
peer1  | {"peer_id":3,"action":"sent","message_type":"prepare_ack","message_value":"X","proposal_num":0}
peer1  | {"peer_id":1,"action":"sent","message_type":"prepare","message_value":"X","proposal_num":0}
peer1  | {"peer_id":4,"action":"sent","message_type":"prepare_ack","message_value":"X","proposal_num":0}
peer4  | {"peer_id":1,"action":"sent","message_type":"prepare","message_value":"X","proposal_num":0}
peer4  | {"peer_id":4,"action":"sent","message_type":"prepare_ack","message_value":"X","proposal_num":0}
peer1  | {"peer_id":1,"action":"sent","message_type":"accept","message_value":"X","proposal_num":0}
peer2  | {"peer_id":1,"action":"sent","message_type":"accept","message_value":"X","proposal_num":0}
peer2  | State updated: accepted_value = X
peer2  | {"peer_id":2,"action":"sent","message_type":"accept_ack","message_value":"X","proposal_num":0}
peer1  | {"peer_id":2,"action":"sent","message_type":"accept_ack","message_value":"X","proposal_num":0}
peer1  | {"peer_id":1,"action":"sent","message_type":"accept","message_value":"X","proposal_num":0}
peer1  | {"peer_id":3,"action":"sent","message_type":"accept_ack","message_value":"X","proposal_num":0}
peer3  | {"peer_id":1,"action":"sent","message_type":"accept","message_value":"X","proposal_num":0}
peer3  | State updated: accepted_value = X
peer3  | {"peer_id":3,"action":"sent","message_type":"accept_ack","message_value":"X","proposal_num":0}
peer1  | {"peer_id":1,"action":"sent","message_type":"accept","message_value":"X","proposal_num":0}
peer4  | {"peer_id":1,"action":"sent","message_type":"accept","message_value":"X","proposal_num":0}
peer4  | State updated: accepted_value = X
peer4  | {"peer_id":4,"action":"sent","message_type":"accept_ack","message_value":"X","proposal_num":0}
peer1  | {"peer_id":4,"action":"sent","message_type":"accept_ack","message_value":"X","proposal_num":0}
peer1  | State updated: accepted_value = X
peer1  | {"peer_id":1,"action":"chose","message_type":"chose","message_value":"X","proposal_num":0}