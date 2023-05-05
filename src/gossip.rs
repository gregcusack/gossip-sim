use std::path::Iter;

use solana_sdk::blake3::Hash;

use {
    crate::{push_active_set::PushActiveSet, received_cache::ReceivedCache, Error},
    crossbeam_channel::{Receiver, Sender},
    itertools::Itertools,
    rand::Rng,
    log::{error, info},
    solana_client::{
        rpc_client::RpcClient, rpc_config::RpcGetVoteAccountsConfig,
        rpc_response::RpcVoteAccountStatus,
    },
    solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey},
    std::{
        collections::{HashMap, HashSet, VecDeque},
        sync::Arc,
        time::{Instant},
        str::FromStr,
    },
};

pub(crate) const CRDS_UNIQUE_PUBKEY_CAPACITY: usize = 8192;
const GOSSIP_PUSH_FANOUT: usize = 6;

// pub struct ClusterNode {
//     pubkey: Pubkey,
//     distance: u64,
// }

pub struct Cluster {
    // count: u64,

    // set of nodes that have allready been visited!
    visited: HashSet<Pubkey>,

    // keep track of the nodes that still need to be explored.
    queue: VecDeque<Pubkey>,

    // keep track of the shortest distance from the starting node to each node in the graph
    distances: HashMap<Pubkey, u64>,

    // keep track of the order in which each recipient node is reach by its neighbors
    // key is the recipient node
    // value hashmap represents the neighbors of the recipient node as the key
    // and the value is the order in which each node was reached by its neighbor
    orders: HashMap<Pubkey, HashMap<Pubkey, Vec<u64>>>,
}

impl Cluster {

    pub fn new() -> Self {
        Cluster { 
            // count: 0,
            visited: HashSet::new(),
            queue: VecDeque::new(),
            distances: HashMap::new(),
            orders: HashMap::new(),
        }
    }


    pub fn print_results(
        self,
    ) {
        info!("DISTANCES FROM ORIGIN");
        for (pubkey, hops) in self.distances {
            info!("dest node, hops: ({:?}, {})", pubkey, hops);
        }
        info!("----------------------------------------------");
        info!("NODE ORDERS");
        for (recv_pubkey, neighbors) in self.orders {
            info!("recv_pubkey: {:?}", recv_pubkey);
            for (peer, order) in neighbors {
                info!("neighbor pubkey: {:?}", peer);
                info!("order: ");
                for o in order {
                    info!("{}", o)
                }

            }
        }

    }

    fn initize(
        &mut self,
        stakes: &HashMap<Pubkey, u64>,
    ) {
        for (pubkey, _) in stakes {
            // Initialize the `distances` hashmap with a distance of infinity for each node in the graph
            self.distances.insert(*pubkey, u64::MAX);
            // self.orders.insert(*pubkey, HashMap::new());
            // self.orders.insert(*pubkey, vec![vec![]; GOSSIP_PUSH_FANOUT]);
        }
    }

    pub fn new_mst(
        &mut self,
        origin_pubkey: &Pubkey,
        stakes: &HashMap<Pubkey, u64>,
        node_map: &HashMap<Pubkey, &Node>,
    ) {
        //initialize BFS setup
        self.initize(stakes);

        // initialize for origin node
        self.distances.insert(*origin_pubkey, 0); //initialize beginning node
        self.queue.push_back(*origin_pubkey);
        self.visited.insert(*origin_pubkey);

        // going through BFS
        while !self.queue.is_empty() {
            // Dequeue the next node from the queue and get its current distance
            let current_node_pubkey = self.queue.pop_front().unwrap();
            let current_distance = self.distances[&current_node_pubkey];

            // need to get the actual node itself so we can get it's active_set and PASE
            let current_node = node_map.get(&current_node_pubkey).unwrap();

            // For each peer of the current node's PASE (limit PUSH_FANOUT), 
            // update its distance and add it to the queue if it has not been visited
            for neighbor in current_node
                .active_set
                .get_nodes(
                    &current_node.pubkey(), 
                    &origin_pubkey, 
                    |_| false, 
                    stakes
                )
                .take(GOSSIP_PUSH_FANOUT) {
                    //check if we have visited this node before.
                    if !self.visited.contains(neighbor) {
                        // if not, we insert it
                        self.visited.insert(*neighbor);
                        //update the distance. saying the neighbor node we are looking at is
                        // an additional hop from the current node. so it is going to be 
                        // an additional hop
                        self.distances.insert(*neighbor, current_distance + 1);
                        
                        // Initialize the `orders` hashmap for the neighbor
                        self.orders.insert(*neighbor, HashMap::new());

                        // add current node to the orders hashmap 
                        // for this neighbor, it's neighbor it the current node
                        // and the distances is the distance to the neighbor...don't understand...
                        self.orders
                            .get_mut(neighbor)
                            .unwrap()
                            .insert(
                                current_node_pubkey, 
                                vec![self.distances[neighbor]]
                            );
                        self.queue.push_back(*neighbor);
                    } else if self.distances[neighbor] == current_distance + 1 {
                        // If the neighbor has already been visited and its distance is the same as the current distance + 1,
                        // add the current node to its `orders` hashmap
                        self.orders
                            .get_mut(neighbor)
                            .unwrap()
                            .entry(current_node_pubkey)
                            .or_insert_with(|| vec![])
                            .push(self.distances[neighbor]);
                    }
            }
        }
    
    }






    pub fn start_mst(
        &mut self, 
        origin_pubkey: &Pubkey,
        stakes: &HashMap<Pubkey, u64>,
        node_map: &HashMap<Pubkey, &Node>,
    ) {
        info!("In start_mst(). origin pk:");
        let origin: &Node = node_map.get(origin_pubkey).unwrap();
        let curr_node = origin;
        self.visited.insert(curr_node.pubkey());
        for node_pubkey in origin
            .active_set
            .get_nodes(&curr_node.pubkey(), &origin_pubkey, |_| false, stakes)
            .take(GOSSIP_PUSH_FANOUT) {
                let node = node_map.get(node_pubkey).unwrap();
                if !self.visited.contains(node_pubkey) {
                    self.run_mst(node, origin_pubkey, stakes, node_map);
                } else {
                    info!("start_mst: already ran run_mst on node: {:?}", node_pubkey);
                }
        }
    }

    pub fn run_mst(
        &mut self,
        current_node: &Node,
        origin_pubkey: &Pubkey,
        stakes: &HashMap<Pubkey, u64>,
        node_map: &HashMap<Pubkey, &Node>,
    ) {
        info!("In run_mst(). pk: {:?}", current_node.pubkey());
        // self.incr_count();
        // if self.count() > 10u64 {
        //     return
        // }
        self.visited.insert(current_node.pubkey());
        for node in current_node
            .active_set
            .get_nodes(&current_node.pubkey(), &origin_pubkey, |_| false, stakes)
            .take(GOSSIP_PUSH_FANOUT) {
                let node = node_map.get(&node).unwrap();
                if !self.visited.contains(&node.pubkey()) {
                    self.run_mst(node, origin_pubkey, stakes, node_map)
                } else {
                    info!("run_mst: already ran run_mst on node: {:?}", &node.pubkey());
                }
        }
    }


}


pub struct Node {
    clock: Instant,
    num_gossip_rounds: usize,
    pubkey: Pubkey,
    stake: u64,
    table: HashMap<CrdsKey, CrdsEntry>,
    // active set: PushActiveSet {} Keys are gossip nodes to push messages to.
    // Values are which origins the node has pruned.
    active_set: PushActiveSet,
    received_cache: ReceivedCache,
    receiver: Receiver<Arc<Packet>>,
}

impl Node {
    pub fn pubkey(&self) -> Pubkey {
        self.pubkey
    }

    pub fn stake(&self) -> u64 {
        self.stake
    }

    pub fn table(&self) -> &HashMap<CrdsKey, CrdsEntry> {
        &self.table
    }

    pub fn num_gossip_rounds(&self) -> usize {
        self.num_gossip_rounds
    }

    pub fn run_gossip<R: Rng>(
        &mut self,
        rng: &mut R,
        stakes: &HashMap<Pubkey, u64>,
    )  {
        self.rotate_active_set(rng, 6usize, stakes);
    } 

    fn rotate_active_set<R: Rng>(
        &mut self,
        rng: &mut R,
        gossip_push_fanout: usize,
        stakes: &HashMap<Pubkey, u64>,
    ) {
        // Gossip nodes to be sampled for each push active set.
        // TODO: this should only be a set of entrypoints not all staked nodes.
        let nodes: Vec<_> = stakes
            .keys()
            .copied()
            .chain(self.table.keys().map(|key| key.origin))
            .filter(|pubkey| pubkey != &self.pubkey)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let cluster_size = nodes.len();
        // note the gossip_push_fanout * 3 is equivalent to CRDS_GOSSIP_PUSH_ACTIVE_SET_SIZE in solana
        // note, here the default is 6*3=18. but in solana it is 6*2=12
        self.active_set
            .rotate(rng, gossip_push_fanout * 2, cluster_size, &nodes, stakes, self.pubkey());
    }

    pub fn hey (
        &self,
        stakes: &HashMap<Pubkey, u64>,
    ) {
        let k = self.start_run_mst(stakes);

    }

    pub fn start_run_mst(
        &self, 
        stakes: &HashMap<Pubkey, u64>,
    ) -> Vec<Pubkey> { 
        // let origin = &self.pubkey();
        // TODO: not efficient. just a fix for now. copies every pubkey in the PASE and returns it in a vector
        // it is only GOSSIP_PUSH_FANOUT to copy but still
        return self
            .active_set
            .get_nodes(&self.pubkey(), &&self.pubkey(), |_| false, stakes)
            .take(GOSSIP_PUSH_FANOUT)
            .cloned()
            .collect::<Vec<_>>();

    }

    pub fn run_mst(
        &mut self, 
        stakes: &HashMap<Pubkey, u64>,
        nodes: &Vec<Pubkey>,
        origin: &Pubkey,
    ) {
        info!("hey from node: {:?}", self.pubkey);

    }


}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CrdsKey {
    origin: Pubkey,
    index: usize,
}

#[derive(Debug, Default)]
pub struct CrdsEntry {
    ordinal: u64,
    num_dups: u8,
}

#[derive(Clone)]
pub enum Packet {
    Push {
        from: Pubkey,
        key: CrdsKey,
        ordinal: u64,
    },
    Prune {
        from: Pubkey,
        origins: Vec<Pubkey>,
    },
}

fn make_gossip_cluster(
    accounts: &HashMap<String, u64>
) -> Result<Vec<(Node, Sender<Arc<Packet>>)>, Error> {
    info!("num of node pubkeys in vote accounts: {}", accounts.len());
    let now = Instant::now();

    let nodes: Vec<_> = accounts.into_iter().map(|node| {
        let stake = accounts.get(node.0).copied().unwrap_or_default(); //get stake from 
            let pubkey = Pubkey::from_str(&node.0)?;
            let (sender, receiver) = crossbeam_channel::unbounded();
            let node = Node {
                clock: now,
                num_gossip_rounds: 0,
                stake,
                pubkey,
                table: HashMap::default(),
                active_set: PushActiveSet::default(),
                received_cache: ReceivedCache::new(2 * CRDS_UNIQUE_PUBKEY_CAPACITY),
                receiver,
            };
            Ok((node, sender))
        })
        .collect::<Result<_, Error>>()?;

    let num_nodes_staked = nodes
    .iter()
    .filter(|(node, _sender)| node.stake != 0)
    .count();
    info!("num of staked nodes in cluster: {}", num_nodes_staked);
    info!("num of cluster nodes: {}", nodes.len());
    let active_stake: u64 = accounts.values().sum();
    let cluster_stake: u64 = nodes.iter().map(|(node, _sender)| node.stake).sum();
    info!("active stake:  {}", active_stake);
    info!("cluster stake: {}", cluster_stake);
    Ok(nodes)
}   

#[allow(clippy::type_complexity)]
pub fn make_gossip_cluster_from_map(
    accounts: &HashMap<String, u64>
) -> Result<Vec<(Node, Sender<Arc<Packet>>)>, Error> {
    make_gossip_cluster(&accounts)
}

#[allow(clippy::type_complexity)]
pub fn make_gossip_cluster_from_rpc(
    rpc_client: &RpcClient,
) -> Result<Vec<(Node, Sender<Arc<Packet>>)>, Error> {
    let config = RpcGetVoteAccountsConfig {
        vote_pubkey: None,
        commitment: Some(CommitmentConfig::finalized()),
        keep_unstaked_delinquents: Some(true),
        delinquent_slot_distance: None,
    };
    //Pull vote accounts from mainnet (this is default. can set via command line args)
    let vote_accounts: RpcVoteAccountStatus = rpc_client.get_vote_accounts_with_config(config)?;
    info!(
        "num of vote accounts: {}",
        vote_accounts.current.len() + vote_accounts.delinquent.len()
    );
    //get map of node stakes (Node Pubkey => stake)
    let stakes: HashMap</*node pubkey:*/ String, /*activated stake:*/ u64> = vote_accounts
        .current
        .iter()
        .chain(&vote_accounts.delinquent)
        .into_grouping_map_by(|info| info.node_pubkey.clone())
        .aggregate(|stake, _node_pubkey, vote_account_info| {
            Some(stake.unwrap_or_default() + vote_account_info.activated_stake)
        });
    
    make_gossip_cluster(&stakes)
}