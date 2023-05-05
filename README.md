# gossip-sim
A Simulation Framework for Solana's Gossip Protocol

## Goal
- Simulate the core components and functionality of Solana's gossip protocol
- Measure tree coverage based on message origin
    - At time `t` what does the spanning tree for messages originating from `Node A` look like?
    - What percentage of nodes in the network will receive this message with push messages?
- Test pruning logic
- Test scoring logic
- Test shuffle logic
    - 

## Questions to be answered
- Do low staked nodes ever get starved (no one pushes to them/not a part of a spanning tree)
- 

## Configurable Parameters
- Number of nodes to sim
- Stake of nodes


## Progress
- [x] Initialize all node with respective stakes and simulated active_sets
- [x] Given a message from an origin and all nodes' stakes and active sets, track a message throughout the network
- [x] Determine the minimum number of hops the message takes to reach all nodes in the network
- [x] For a given destination node and its inbound neighbors, determine which neighbor was first, second, third, etc to deliver the message
- [x] Determine coverage of network. # of nodes Rx message / # of nodes in network.