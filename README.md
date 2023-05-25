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


## How to run
#### Option 1: Write keys to file for small tests
- Run the following where `<num-nodes>` is the number of nodes to write to a yaml file at `<path-to-yaml-file>`
    - Note: this method by default will pull from solana mainnet and get the `pubkey: stake` pairs and write them to a file
```
cargo run --bin write-accounts -- 
    --num-nodes <num-nodes> 
    --account-file <path-to-yaml-file>
    --filter-zero-staked-nodes
```

#### Option 2: Read accounts from file and run simulation
- Read accounts from the file you created in (Option 1) and simulate the network
    - Note it will simulate all of the accounts in the file. 
```
cargo run --bin gossip-sim --
    --account-file <path-to-yaml-file> 
    --accounts-from-yaml 
    --push-fanout <push_fanout> 
    --active-set-size <active_set_size> 
    --iterations <number_of_gossip_iterations> 
    --origin-rank <origin_stake_rank> 
    --rotation-probability <probability-of-active-set-rotation> 
    --min-ingress-nodes <min-ingress-nodes> 
    --stake-threshold <min-stake-threshold>
    --filter-zero-staked-nodes
    --num-buckets <num-buckets-for-histogram>
```
#### Option 3: Pull accounts from mainnet and run simulation
- This will pull all node accounts from mainnet and simulate the network.
```
cargo run --bin gossip-sim --
    --push-fanout <push_fanout> 
    --active-set-size <active_set_size> 
    --iterations <number_of_gossip_iterations> 
    --origin-rank <origin_stake_rank> 
    --rotation-probability <probability-of-active-set-rotation> 
    --min-ingress-nodes <min-ingress-nodes> 
    --stake-threshold <min-stake-threshold>
    --filter-zero-staked-nodes
    --num-buckets <num-buckets-for-histogram>
```

#### Option 4: Pull accounts, run multiple simulations, change specific config param
- This will pull all node accounts from mainnet and simulate the network.
```
cargo run --bin gossip-sim --
    --push-fanout <push_fanout> 
    --active-set-size <active_set_size> 
    --iterations <number_of_gossip_iterations> 
    --origin-rank <origin_stake_rank> 
    --rotation-probability <probability-of-active-set-rotation> 
    --min-ingress-nodes <min-ingress-nodes> 
    --stake-threshold <min-stake-threshold>
    --filter-zero-staked-nodes
    --num-buckets <num-buckets-for-histogram>
    --test-type <test-type>
    --num-simulations <num-simulations>
    --step-size <step-size>
```
`test-type` is an Enum that can be set to one of the following:
```
active-set-size
push-fanout
min-ingress-nodes
min-stake-threshold
origin-rank
[default: no-test]
```
So if `test-type` is set to `active-set-size`, the first simulation will run gossip with an `active-set-size` of `--active-set-size <active_set_size>` passed in. 
On each simulation run, the `active-set-size` will increment by `step-size`. 
The number of times a simulation is run (and `active-set-size` incremented) is defined by `num-simulations`.
Default if `test-type` is not set is `no-test`. Meaning everything will run just once as before (so would be same as Option 2 or Option 3 above)



## Interpreting the output
Prints out coverage, RMR, Aggregate Hop info, LDH, stranded nodes
- Coverage:
    - Number of nodes that received a message from an origin / Number of nodes in the network
    - Mean, Median, Max, Min across all gossip iterations
- RMR (Relative Message Redundancy)
    - `RMR = m / (n-1) - 1, for n > 1`
    - m: total number of payload messages exchanged during gossip (push and prune)
    - n: total number of nodes that received the message
    - Mean, Median, Max, Min across all gossip iterations
- Aggregate Hop Info
    - Number of hops required to deliver a message 
    - Mean, Median, Max across all gossip iterations
- LDH (Last Delivery Hop)
    - Max number of hops required to transmit a message from an origin
    - Mean, Median, Max, Min across all gossip iterations
- Stranded Nodes
    - Identifies when a node did not receive a message from an origin
    - Pubkey and stake of stranded nodes and how many times (gossip iterations) that node was stranded

```
|------------------------|
|---- COVERAGE STATS ----|
|------------------------|
Number of iterations: 400
Coverage Mean: 0.984000
Coverage Median: 0.983333
Coverage Max: 0.996667
Coverage Min: 0.960000
|-------------------------------------------------|
|---- RELATIVE MESSAGE REDUNDANCY (RMR) STATS ----|
|-------------------------------------------------|
Number of iterations: 400
RMR Mean: 3.107014
RMR Median: 2.202361
RMR Max: 10.041812
RMR Min: 1.836177
|---------------------------------|
|------ AGGREGATE HOP STATS ------|
|---------------------------------|
Aggregate Hops Mean: Mean: 4.497764
Aggregate Hops Median: Median: 4.00
Aggregate Hops Max: Max: 11
|-------------------------------------|
|------ LAST DELIVERY HOP STATS ------|
|-------------------------------------|
LDH Mean: Mean: 9.455000
LDH Median: Median: 9.00
LDH Max: Max: 11
LDH Min: Min: 7
|----------------------------------------------------------|
|---- STRANDED NODES (Pubkey, stake, # times stranded) ----|
|----------------------------------------------------------|
Total stranded nodes: 41
BiSnfKPz671sajgRcvDbWEa6jG41WQFwTyPgQsSQb1qL,	0,		        106
9T6SNsBimjCRJpkEjiVsc8AcxTBa1XVA7RjnBGGfWP23,	75166146859914,	110
GKNG9M61nZmuEcUou4bMnonKh24wY1sNDYWp88ig2gRn,	2000000,	    236
F5vxLRkYehYWEKPAUyph36EFakpXbsZDBLqysEsy8E8Q,	0,		        10
73hojLdq1vZDSxeVQEqVFJ4iwLngdvEJPEpEHkSdv6BZ,	307649174851299,28
...
7QYESzKZ1qttXDND58pWPpH5tBhEcwBWxa6GCJNTugF7,	105137195473920,32

```

## Progress
- [x] Initialize all node with respective stakes and simulated active_sets
- [x] Given a message from an origin and all nodes' stakes and active sets, track a message throughout the network
- [x] Determine the minimum number of hops the message takes to reach all nodes in the network
- [x] For a given destination node and its inbound neighbors, determine which neighbor was first, second, third, etc to deliver the message
- [x] Determine coverage of network. # of nodes Rx message / # of nodes in network.
- [x] Measure Relative Message Redundancy (RMR)
- [x] Measure Last Hop Delivery (LHD)
- [x] Identify MST src->vec<dest>
- [x] Identify dest->vec<src> prunes
- [x] write tests for above
- [x] implement pruning logic.
- [x] measure coverage for different `CRDS_GOSSIP_PUSH_FANOUT` and `CRDS_GOSSIP_PUSH_ACTIVE_SET_SIZE`
- [x] maintain minimum stake threshold and minimum incoming connections when pruning

## Caveat
- We do not simulate pull requests

## Below output can be printed out using a Debug log level.
- Prints out all of the account pubkeys and stakes. e.g.
```
...
pubkey, stake: CQBZxCuQsuufvvDAotfTR5q9Rd3YyYRD3E2jxrryQH5n, 2000000
pubkey, stake: G21p5Lt2Y5AWkNWjkEz1sxS9bDEeKm7vvZb7m9BEQ2MH, 2000000
pubkey, stake: e1afFaogyDznnUBMWQbPgnZ3VUzkGVsEhyxBPH13BVx, 92932630370
pubkey, stake: HoZKMBNVhY8uiYK5NtVo6V4pnjMg3bboFKx6SCZob5N7, 77808508023217
pubkey, stake: 5EjTEuKQto7ZzTuc347NuY7PDkhqLA85uxSzou2TbDG9, 2000000
...
```
- Runs simulation
- Tells you the pubkey of the origin node it used for the simulation
- Outputs the minimum hop distances from the origin. Where the pubkey is the destination node and the integer to the right is the minimum number of hops it took to reach the destination node.
    - Note, if the number of hops is `18446744073709551615` that is `u64::MAX` and means the node has been stranded in this iteration
```
DISTANCES FROM ORIGIN
dest node, hops: (52BhGzuncmZfRLyhHzTE3ynvRK2sNAD1uQvqcjFiNFic, 3)
dest node, hops: (Aw5wEMXhbygFLR7jHtHpih8QvxVBGAMTqsQ2SjWPk1ex, 4)
dest node, hops: (22QuxUxG2eZcPsRgRTEA5VJMEFBJFWRTm5oGBqZjRMs1, 3)
dest node, hops: (3ScqKCyAKGN4B27S1mFNCCna4cf3ZBZf6diuXNPq8pBq, 3)
dest node, hops: (FrbHEfpqGGUNrverNM7KMqeBTjmRterxVg3xAZdFNdPB, 3)
...
```
- Next is the number of hops it takes to get to a destination node through its inbound peers. So below the first destination node is `5Reg...` and it has 3 inbound peers. It took 4 hops to get to `5R3g...` through `8xdf...`, 3 through `DSRV...`, and 2 through `7CyN...`. For the next iteration of this work, we now know `5R3g...` can prune `8Xdf` and `DSRV`.
    - If the number of hops are the same, then in the future we can just select one at random to prune. Assuming zero network delay
```
NODE ORDERS
----- dest node, num_inbound: 5Regk6KStwEfFUMs5hLixNwBtC476Ww7ag5MxrimzbrA, 3 -----
neighbor pubkey, order: 8XdfzCAkynSsvDqUi52Nadh4zwdbEUZt6jTWgdDRT2hP, 4
neighbor pubkey, order: DSRVdh9PQaqAcFtMCbJhyD4yMD5H2EeHNzdbqWctRY4E, 3
neighbor pubkey, order: 7CyNBLaoav9fZhX4D2WGrL5XCuMroSgDut68vtL8NB9p, 2
----- dest node, num_inbound: Hqc2qT3vXvBSKozmz7Rd7gLF6jUJHuQEGBTnCHzEBnqk, 5 -----
neighbor pubkey, order: 9Xm2WtKW1tEpY5wxKZD9XbAmojHGGtjiGeM3LotYT1Z9, 5
neighbor pubkey, order: 7Jpqy46cqsMRqdyiwqhhmYHVjhML3T4hRAP2qFpeK67b, 4
neighbor pubkey, order: GPc2LPuZf1exmn1jDe7gzkM6jQ4NCcuh99tC8pzAFocb, 5
neighbor pubkey, order: 3bQiAe7Hdm1MqUnoV6BsFahiTFa9Va9zM2BoRN5Hqkzc, 5
neighbor pubkey, order: AXrSStSdmJuEubQx5bYx7aEDZ1zmt4CjrHvEmTNNenkV, 4
...
```
- Outputs the cluster coverage. aka the percentage of nodes receiving messages for this origin and current set of active sets
- Outputs the number of standed nodes

#### MST Adjacency List
```
MST:
\##### src: CsQpiAH9i1uJwAPaQRZJXzYmDJPar4gJsi9Xwr7JLk9L #####
dest: 5uAJn8Wfie7k7kd6RzCutG8JSAxxHS6b7iRriaEBp3Cc
\##### src: 4ZToBgveZ5m8NySrDyPA2fiGVRVBioaoMXD31KGidm65 #####
dest: 8cuMnfEfiaJfWsLj7pPhtSvf9dxXs2eHG55CJmp1bzJP
dest: J5wNgFnrLQiRLHEzySuYphYpz7cQsTmwkVWxEKPJcLWe
\##### src: 7CjTgWwwvQ1VjSsNPWN6sKFpTrziD2yJpcr6KaN1jGFG #####
dest: 8D5rQbJD9qLNJm9HyTjFWV93CBc29ozAdGUia4hyMhw
dest: 9up7cNyP6c9Ay3bwNhLsdMUu44tNLosXHGhb1M6kjZ8D
dest: 6Jqn61UPZmAskQSLBv3eDpa6cKkZKBkVtqDVGcaakwEa
```
- The outputs the adjacency list of the network's MST
```
CsQp -> 5uAJ
4ZTo -> 8cuM, J5wN
7CjT -> 8d5r, 9up7, 6Jqn
```

#### MST Prunes
```
--------- Pruner: FwdQr8NUppcFZFBNgV2V7fzANSuHnU3BebKYzh2U54G2 ---------
Prunee: 6iVYxcvw9ctBwRb4xspzNXE4x5ykH6LPZJaGYTSPoZRa
--------- Pruner: 6XjxbDk9epumcbnsq35NVGytzb5b9aHPodWpNnfUKaC5 ---------
Prunee: DLiBQUytXnuXTuhhN8Mkom7rHbA9L49yiLchVBr2qDUp
Prunee: FJfh3XUydWdBtrVTafcTyQfsgrpcggr1DJMGddKapTsL
 --------- Pruner: D6uUDTEgXDf1yzLuQfFFCEKF9a2Ri5trFAWwaUpKB2ji ---------
Prunee: 29rzUXiy2kYridD6zxc6nszsQpgVW8bknW9NMQEiQThi
Prunee: 7TtboPzuUFJg5gCjnPVJKmBRZhfEmoAnNWXdsX81N28T
Prunee: J4izU5SytdEsvEfotPJxvHMqz2CPkSBGMdQCGgT4XxYP
```
- Outputs the list of prunes by pruner => prunees
```
FwdQ --prunes--> 6iVY
6Xjx --prunes--> DLiB, FJfh
D6uU --prunes--> 29rz, 7Ttb, J4iz
```
- Note pruning is done on a second come, first prune basis. If you send a message to a node that has already received the message even if its the same number of hops, the received will prune you.
- In example below, C will prune D.
```
A -> C
B -> D -> C
```
- In example below, C will prune D. Had to make an assumption here on which message C would receive first with same number of hops. so for this simulation, we just take the first on the BFS alg.
```
A -> C
D -> C
```



