use {
    crate::{push_active_set::PushActiveSet, received_cache::ReceivedCache, Error},
    crossbeam_channel::{Receiver, Sender},
    itertools::Itertools,
    rand::Rng,
    log::{debug, info},
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
    // and the value is the number of hops it took for that node to deliver the message
    // to the main node.
    // So A => {B => 4}. Would mean for destination A and A's neighbor B.
    // It took 4 hops to reach A through B.
    // So A => {C => 3}. Would mean for dest A and A's neighbor C.
    // It took 3 hops to reach A through C.
    // For our next step we would need to PRUNE B.
    orders: HashMap<Pubkey, HashMap<Pubkey, u64>>,
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

    pub fn coverage(
        &self,
        stakes: &HashMap<Pubkey, u64>,
    ) -> (f64, usize) {
        debug!("visited len, stakes len: {}, {}", self.visited.len(), stakes.len());
        (self.visited.len() as f64 / stakes.len() as f64, stakes.len() - self.visited.len())
    }

    pub fn print_hops(
        &self,
    ) {
        info!("DISTANCES FROM ORIGIN");
        for (pubkey, hops) in &self.distances {
            info!("dest node, hops: ({:?}, {})", pubkey, hops);
        }
    }

    // print the order in which a node receives a message from it's inbound peers
    // A => {B => 4}
    // A => {C => 3}
    // A received a message in 4 hops through B
    // A received a message in 3 hops through C
    pub fn print_node_orders(
        &self,
    ) {
        info!("NODE ORDERS");
        for (recv_pubkey, neighbors) in &self.orders {
            info!("----- dest node, num_inbound: {:?}, {} -----", recv_pubkey, neighbors.len());
            for (peer, order) in neighbors {
                info!("neighbor pubkey, order: {:?}, {}", peer, order);
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
                        
                        self.queue.push_back(*neighbor);
                    }

                    // Here we track, for specific neighbor, we know that the current node
                    // has sent a message to the neighbor. So we must note that
                    // our neighbor has received a message from the current node
                    // and it took current_distance + 1 hops to get to the neighbor
                    if !self.orders.contains_key(neighbor) {
                        self.orders.insert(*neighbor, HashMap::new());
                    }
                    self.orders
                        .get_mut(neighbor)
                        .unwrap()
                        .insert(current_node_pubkey, current_distance + 1);
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