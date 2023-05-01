# gossip-sim
A Simulation Framework for Solana's Gossip Protocol

## Goal
- Simulate the core component's and functionality of Solana's gossip protocol
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