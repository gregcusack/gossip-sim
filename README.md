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

## Overview
1) Pull `num-nodes` from `JSON_RPC` and write `pubkey: stake` to a stake account file.  Use `write-accounts` bin
2) For experiment `gossip-sim`,  can either read in accounts from stake account file or pull from `JSON_RPC`
3) From accounts run a single round of gossip to initialize all node's active sets
4) Given an origin, we can simulate the path that origin's message would traverse through the network. We identify the minimum hop count required to reach every node in the network.
5) For each destination node, we maintain the node's inbound connections. We track the number of hops it takes to get to the node through each of its neighbors. For example, in the adjacency list below,  `A`, `D`, and `G` will all send a message to `B`. The shortest path to `B` will be preserved as 1 hop: `A->B`. We can also tell that it took 4 hops through `D` and 5 hops through `G` to reach `B`. 

So, For the next step, we can use `B`'s inbound connection hop count to prune both `D `and `G`.

For a directed adjacency list (found via a specific origin and simulated active sets),
```
A -> B
B -> C
C -> D, F
D -> B
F -> G
G -> B
```

Graph visual:
```
          A
          |
          v
     ---> B <---
    |     |     |
    |     v     |
    |     C --> D 
    |     |
    |     v
    G <-- F
```

6) After the simulation we can see the coverage of the network.


## Progress
- [x] Initialize all node with respective stakes and simulated active_sets
- [x] Given a message from an origin and all nodes' stakes and active sets, track a message throughout the network
- [x] Determine the minimum number of hops the message takes to reach all nodes in the network
- [x] For a given destination node and its inbound neighbors, determine which neighbor was first, second, third, etc to deliver the message
- [x] Determine coverage of network. # of nodes Rx message / # of nodes in network.

## How to run
#### Option 1: Write keys to file for small tests
- Run the following where `<num-nodes>` is the number of nodes to write to a yaml file at `<path-to-yaml-file>`
    - Note: this method by default will pull from solana mainnet and get the `pubkey: stake` pairs and write them to a file
```
cargo run --bin write-accounts -- --num-nodes <num-nodes> --account-file <path-to-yaml-file>
```

#### Option 2: Read accounts from file and run simulation
- Read accounts from the file you created in (Option 1) and simulate the network
    - Note it will simulate all of the accounts in the file. 
```
cargo run --bin gossip-sim -- --account-file `<path-to-yaml-file>` --accounts-from-yaml
```
#### Option 3: Pull accounts from mainnet and run simulation
- This will pull all node accounts from mainnet and simulate the network. It will use zero-staked nodes.
```
cargo run --bin gossip-sim
```


## Questions to be answered
- Do low staked nodes ever get starved (no one pushes to them/not a part of a spanning tree)

## Configurable Parameters
- Number of nodes to sim
- Stake of nodes

