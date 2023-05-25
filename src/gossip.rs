use {
    crate::{push_active_set::PushActiveSet, received_cache::ReceivedCache, Error, gossip_stats},
    crossbeam_channel::{Receiver, Sender},
    itertools::Itertools,
    rand::Rng,
    log::{debug, info, error},
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
        fs::File,
        io::{BufWriter, Write},
        iter::repeat,
    },
    rand::rngs::StdRng,
};

#[cfg_attr(test, cfg(test))]
pub(crate) const CRDS_UNIQUE_PUBKEY_CAPACITY: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Testing {
    ActiveSetSize,
    PushFanout,
    MinIngressNodes,
    MinStakeThreshold,
    OriginRank,
    NoTest,
}

impl std::fmt::Display for Testing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Testing::ActiveSetSize => write!(f, "ActiveSetSize"),
            Testing::PushFanout => write!(f, "PushFanout()"),
            Testing::MinIngressNodes => write!(f, "MinIngressNodes()"),
            Testing::MinStakeThreshold => write!(f, "MinStakeThreshold()"),
            Testing::OriginRank => write!(f, "OriginRank()"),
            Testing::NoTest => write!(f, "NoTest()"),
        }
    }
}

impl FromStr for Testing {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active-set-size" => Ok(Testing::ActiveSetSize),
            "push-fanout" => Ok(Testing::PushFanout),
            "min-ingress-nodes" => Ok(Testing::MinIngressNodes),
            "min-stake-threshold" => Ok(Testing::MinStakeThreshold),
            "origin-rank" => Ok(Testing::OriginRank),
            "no-test" => Ok(Testing::NoTest),
            _ => Err(format!("Invalid test type: {}", s)),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum StepSize {
    Integer(usize),
    Float(f64),
}

impl From<StepSize> for usize {
    fn from(value: StepSize) -> Self {
        match value {
            StepSize::Integer(num) => num,
            StepSize::Float(num) => num as usize,
        }
    }
}

impl From<StepSize> for f64 {
    fn from(value: StepSize) -> Self {
        match value {
            StepSize::Integer(num) => num as f64,
            StepSize::Float(num) => num,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Config<'a> {
    pub gossip_push_fanout: usize,
    pub gossip_active_set_size: usize,
    pub gossip_iterations: usize, 
    pub accounts_from_file: bool,
    pub account_file: &'a str,
    pub origin_rank: usize,
    pub probability_of_rotation: f64,
    pub prune_stake_threshold: f64,
    pub min_ingress_nodes: usize,
    pub filter_zero_staked_nodes: bool,
    pub num_buckets_for_stranded_node_hist: u64,
    pub test_type: Testing,
    pub num_simulations: usize,
    pub step_size: StepSize,
}

pub struct Cluster {
    gossip_push_fanout: usize,

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

    // store the adjacency list the MST. src_pubkey => HashSet<dst_pubkey>
    mst: HashMap<Pubkey, HashSet<Pubkey>>, 

    // get all nodes a src is pushing too
    // src_node => dst_nodes {A, B, C, ..., N}
    pushes: HashMap<Pubkey, HashSet<Pubkey>>,

    rmr: gossip_stats::RelativeMessageRedundancy,

    // prunes_v2: self_pubkey => peer => vec<origin>
    // aka who (peer) sent us (self_pubkey) the extra message. and who (origin) created that extra messages
    // pruner => prunee => Vec<origin>. pruner creates prune message. Prunee processes that prune
    // message, and stops sending messages to pruner.
    prunes: HashMap<Pubkey, HashMap<Pubkey, Vec<Pubkey>>>,
}

impl Cluster {

    pub fn new(
        push_fanout: usize
    ) -> Self {
        Cluster { 
            gossip_push_fanout: push_fanout,
            visited: HashSet::new(),
            queue: VecDeque::new(),
            distances: HashMap::new(),
            orders: HashMap::new(),
            mst: HashMap::new(),
            prunes: HashMap::new(),
            pushes: HashMap::new(),
            rmr: gossip_stats::RelativeMessageRedundancy::default(),
        }
    }

    fn clear_maps(
        &mut self,
    ) {
        self.visited.clear();
        self.queue.clear();
        self.distances.clear();
        self.orders.clear();
        self.mst.clear();
        self.prunes.clear();
        self.pushes.clear();
        self.rmr.reset();
    }

    pub fn get_outbound_degree(
        &self,
        src_node: &Pubkey,
    ) -> usize {
        self.mst.get(src_node).unwrap().len()
    }

    pub fn get_num_prunes_by_dest (
        &self,
        dst_node: &Pubkey,
    ) -> Result<usize, ()> {
        match self.prunes.get(dst_node) {
            Some(adjacent_nodes) => Ok(adjacent_nodes.len()),
            None => Err(()),
        }
    }

    pub fn prune_exists (
        &self,
        dst_node: &Pubkey,
        src_node: &Pubkey,
    ) -> Result<bool, ()> {
        if let Some(adjacent_nodes) = self.prunes.get(dst_node) {
            if let Some(nodes) = adjacent_nodes.get(src_node) {
                if !nodes.is_empty() {
                    return Ok(true);
                }
            }
        }
        Err(())
    }

    pub fn edge_exists (
        &self,
        src_node: &Pubkey,
        dst_node: &Pubkey,
    ) -> Result<bool, ()> {
        match self.mst.get(src_node) {
            Some(adjacent_nodes) => Ok(adjacent_nodes.contains(dst_node)),
            None => Err(()),
        }
    }

    pub fn get_order_key(
        &self,
        dest_node: &Pubkey,
    ) -> bool {
        self.orders.contains_key(dest_node)
    }

    pub fn get_num_inbound(
        &self,
        dest_node: &Pubkey,
    ) -> usize {
        self.orders
            .get(dest_node)
            .unwrap()
            .len()
    }

    pub fn get_orders_hops(
        &self,
        dest_node: &Pubkey,
        src_node: &Pubkey,
    ) -> Option<&u64> {
        self.orders
            .get(dest_node)
            .unwrap()
            .get(src_node)
    }

    pub fn get_orders(
        &self,
    ) -> &HashMap<Pubkey, HashMap<Pubkey, u64>> {
        &self.orders
    }

    pub fn get_visited_len(
        &self,
    ) -> usize {
        self.visited.len()
    }

    pub fn get_distance(
        &self,
        pubkey: &Pubkey,
    ) -> u64 {
        *self.distances.get(pubkey).unwrap()
    }

    pub fn coverage(
        &self,
        stakes: &HashMap<Pubkey, u64>,
    ) -> (f64, usize) {
        debug!("visited len, stakes len: {}, {}", self.visited.len(), stakes.len());
        (self.visited.len() as f64 / stakes.len() as f64, stakes.len() - self.visited.len())
    }

    pub fn stranded_nodes(
        &self,
    ) -> Vec<Pubkey> {
        let mut stranded_pubkeys: Vec<Pubkey> = Vec::new();

        for (pubkey, hops) in self.distances.iter() {
            if hops == &u64::MAX {
                stranded_pubkeys.push(*pubkey);
            }
        }

        stranded_pubkeys
    }

    pub fn get_distances(
        &self,
    ) -> &HashMap<Pubkey, u64> {
        &self.distances
    }

    pub fn get_prunes(
        &self,
    ) -> &HashMap<Pubkey, HashMap<Pubkey, Vec<Pubkey>>> {
        &self.prunes
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

    pub fn print_mst(
        &self,
    ) {
        info!("MST: ");
        for (src, dests) in &self.mst {
            info!("##### src: {:?} #####", src);
            for dest in dests {
                info!("dest: {:?}", dest);
            }
        }
    }

    pub fn print_prunes(
        &self,
    ) {
        info!("PRUNES: ");
        for (pruner, prunes) in &self.prunes {
            info!("--------- Pruner: {:?} ---------", pruner);
            for (prunee, _origin) in prunes {
                info!("Prunee: {:?}", prunee);
            }
        }
    }

    pub fn get_pushes(
        &self,
    ) -> &HashMap<Pubkey, HashSet<Pubkey>> {
        &self.pushes
    }

    pub fn print_pushes(
        &self,
    ) {
        info!("PUSHES: ");
        for (src, dests) in &self.pushes {
            info!("************* SRC: {:?}, # {} *************", src, dests.len());
            for dst in dests {
                info!("Dest: {:?}", dst);
            }
        }
    }

    // calculate rmr if not already calculated and return it
    // if calculated return it. 
    pub fn relative_message_redundancy(
        &mut self,
    ) -> Result<f64, String> {
        if self.rmr.rmr() == 0.0 {
            self.rmr.calculate_rmr()
        } else {
            Ok(self.rmr.rmr())
        }
    }

    pub fn get_rmr_struct(
        &self,
    ) -> &gossip_stats::RelativeMessageRedundancy {
        &self.rmr
    }

    pub fn write_adjacency_list_to_file(
        &self,
        filename: &str,
    ) -> std::io::Result<()> {
        let file = File::create(filename)?;
        let mut writer = BufWriter::new(file);

        for (src_node, dst_nodes) in self.mst.iter() {
            // Write the source node
            write!(writer, "{:-4}:", src_node)?;
            
            // Write the destination nodes
            for dst_node in dst_nodes {
                write!(writer, " {:-4}", dst_node)?;
            }

            // End the line
            writeln!(writer)?;
        }

        Ok(())
    }

    fn initialize(
        &mut self,
        stakes: &HashMap<Pubkey, u64>,
    ) {
        self.clear_maps();
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
        self.initialize(stakes);

        // initialize for origin node
        self.distances.insert(*origin_pubkey, 0); //initialize beginning node
        self.queue.push_back(*origin_pubkey);
        self.visited.insert(*origin_pubkey);
        self.rmr.increment_n(); // add origin to rmr node count

        // going through BFS
        while !self.queue.is_empty() {
            // Dequeue the next node from the queue and get its current distance
            let current_node_pubkey = self.queue.pop_front().unwrap();
            let current_distance = self.distances[&current_node_pubkey];

            // need to get the actual node itself so we can get it's active_set and PASE
            let current_node = node_map.get(&current_node_pubkey).unwrap();

            // insert current node into pushes map
            self.pushes.insert(current_node_pubkey, HashSet::new());

            // For each peer of the current node's PASE (limit PUSH_FANOUT), 
            // update its distance and add it to the queue if it has not been visited
            let mut pase_counter: usize = 0;
            for (fanout_count, neighbor) in current_node
                .active_set
                .get_nodes(
                    &current_node.pubkey(), 
                    &origin_pubkey, 
                    |_| false, 
                    stakes
                )
                .take(self.gossip_push_fanout)
                .enumerate() {
                    debug!("curr node, neighbor: {:?}, {:?}", current_node.pubkey(), neighbor);
                    // if current_node_pubkey == Pubkey::from_str("B5GABybkqGaxxFE6bcN6TEejDF2tuz6yREciLhvvKyMp").unwrap() {
                    //     info!("neighbor for KyMp: {:?}", neighbor);
                    // }

                    // add neighbor to current_node pushes map
                    // I think this is the one we care about at all times.
                    // TBH not sure MST is really useful. 
                    // We care about who a node is pushing too.
                    self.pushes
                        .get_mut(&current_node_pubkey)
                        .unwrap()
                        .insert(*neighbor);

                    // Ensure the neighbor hasn't pruned us!
                    match self.prune_exists(neighbor, &current_node_pubkey) {
                        Ok(true) => panic!("Error! we are pushing to a node that should be pruned!"), //neighbor has pruned us
                        Ok(false) => (), // neighbor has pruned someone but not us
                        Err(_) => (), // neighbor hasn't pruned anyone
                    };

                    // add new push message to rmr message count.
                    self.rmr.increment_m();

                    //check if we have visited this node before.
                    if !self.visited.contains(neighbor) {
                        // if not, we insert it
                        self.visited.insert(*neighbor);
                        //update the distance. saying the neighbor node we are looking at is
                        // an additional hop from the current node. so it is going to be 
                        // an additional hop
                        // NOTE: with BFS, there is no chance we will find a shorter path than the path we find here
                        // BFS searches at ever increasing distances from an origin node.
                        self.distances.insert(*neighbor, current_distance + 1);
                        
                        self.queue.push_back(*neighbor);

                        // found a new neighbor for our current node
                        // add the new neighbor to the current node's adjacency hashset
                        self.mst
                                .entry(current_node_pubkey)
                                .or_insert_with(|| HashSet::<Pubkey>::new())
                                .insert(*neighbor);

                        // increment the new node (neighbor) to the rmr node count
                        self.rmr.increment_n();
                    } else {
                        // so above, we increment_m because that is indicating we are sending a new message to a neighbor
                        // but once we send it and it results in a prune, we have to count the responding prune message
                        // so this additional increment_m() is for the return "prune" value
                        self.rmr.increment_m();
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
                    pase_counter = fanout_count;
            }
            if pase_counter + 1 != self.gossip_push_fanout {
                debug!("for src_node {:?}", current_node_pubkey);
                debug!("WARNING: Only pushed to {} nodes instead of the expected {} nodes!", pase_counter + 1, self.gossip_push_fanout);
            }
        }
    }

    // loop through orders map and add incoming messages to our receive cache
    pub fn consume_messages (
        &mut self,
        origin: &Pubkey,
        nodes: &mut Vec<Node>,

    ) {
        // orders could just be a hashmap<pubkey, vec<(pubkey, u64)>>
        // node here is the destination
        for node in nodes {
            if node.pubkey() == *origin {
                continue;
            }
            // sources are sending to node.
            let sources = match self.orders.get(&node.pubkey()) {
                Some(sources) => sources,
                None => {
                    debug!("Node did not receive message, stranded: {}", node.pubkey());
                    continue;
                },
            };
            let mut sorted_hops: Vec<(&Pubkey, &u64)> = sources.iter().collect();
            sorted_hops.sort_by(|&(key1, hops1), &(key2, hops2)| {
                if hops1 == hops2 {
                    key1.to_string().cmp(&key2.to_string())
                } else {
                    hops1.cmp(hops2)
                }
            });
            //call record in the order of fewest to most hops. 
            //each time we run record, we increment num_dups.
            for (count, (src, _)) in sorted_hops.iter().enumerate() {
                debug!("dest, src, count: {:?}, {:?}, {}", node.pubkey(), src, count);
                node.received_cache.record(*origin, **src, count)
            }
        }
    }

    // once we add messages to our receive cache, determine which nodes we need to prune
    // based off of order Rx, min stake thresh, min numb ingress nodes.
    pub fn send_prunes(
        &mut self,
        // origins: impl IntoIterator<Item = Pubkey>,
        origin: Pubkey,
        nodes: &mut Vec<Node>,
        // node: &mut Node,
        prune_stake_threshold: f64,
        min_ingress_nodes: usize,
        stakes: &HashMap<Pubkey, u64>,
    ) {
        for node in nodes {
            // generate prunes for the current node.
            let prunes =
                node.received_cache.prune(
                    &node.pubkey(),
                    origin,
                    prune_stake_threshold,
                    min_ingress_nodes,
                    stakes,
                )
                .zip(repeat(origin))
                .into_group_map();

            //for the current node, add in it's prunes
            // prunes (above) are peer => Vec<origins>
            // so for current node, all of the peers sent messages to the node
            // that were duplicates. And they did that for messages originating
            // at "origin"
            self.prunes
                .insert(node.pubkey(), prunes);
        }

    }

    // pruner has sent prunes to a bunch of nodes. we are going to handle
    // those prunes from the side of the nodes (aka the prunees)
    pub fn prune_connections(
        &mut self,
        // origin: &Pubkey,
        node_map: &HashMap<Pubkey, &Node>,
        // nodes: &mut Vec<Node>,
        stakes: &HashMap<Pubkey, u64>,
    ) {
        // pruner: sending the prunes.
        // prunes: peers and origins. 
        for (pruner, prunes) in &self.prunes {
            // loop through prunes to get the peers that have received prune messages
            // from the pruner.
            debug!("len prunes: {}", prunes.len());
            for (current_pubkey, origins) in prunes {
                // now we switch into the context of the prunee. 
                // prunee now has to process the prune messages. so we do that here
                // prunee is going to update it's active_set and remove the pruner 
                // for the specific origin.
                if let Some(current_node) = node_map.get(current_pubkey) {
                    debug!("For node {:?}, processing prune msg from {:?}", current_pubkey, pruner);
                    current_node.active_set.prune(current_pubkey, pruner, &origins[..], stakes);
                } else {
                    error!("ERROR. We should never reach here. all nodes in prunes_v2 should be in node_map");
                }
            }
        }

    }

    pub fn chance_to_rotate<R: Rng>(
        &self,
        rng: &mut R,
        nodes: &mut Vec<Node>,
        active_set_size: usize,
        stakes: &HashMap<Pubkey, u64>,
        probability_of_rotation: f64,
        seeded_rng: &mut StdRng,

    ) {
        debug!("Rotating Active Sets....");
        for node in nodes {
            if seeded_rng.gen::<f64>() < probability_of_rotation {
                debug!("Rotating Active Set for: {:?}", node.pubkey());
                node.rotate_active_set(rng, active_set_size, stakes, false);
            }
        }
    }


}

pub struct Node {
    _clock: Instant,
    num_gossip_rounds: usize,
    pubkey: Pubkey,
    stake: u64,
    table: HashMap<CrdsKey, CrdsEntry>,
    // active set: PushActiveSet {} Keys are gossip nodes to push messages to.
    // Values are which origins the node has pruned.
    active_set: PushActiveSet,
    received_cache: ReceivedCache,
    _receiver: Receiver<Arc<Packet>>,
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
        active_set_size: usize,
        test: bool
    )  {
        self.rotate_active_set(rng, active_set_size, stakes, test);
    } 

    fn rotate_active_set<R: Rng>(
        &mut self,
        rng: &mut R,
        active_set_size: usize,
        stakes: &HashMap<Pubkey, u64>,
        test: bool
    ) {
        // Gossip nodes to be sampled for each push active set.
        // TODO: this should only be a set of entrypoints not all staked nodes.
        let mut nodes: Vec<_> = stakes
            .keys()
            .copied()
            .chain(self.table.keys().map(|key| key.origin))
            .filter(|pubkey| pubkey != &self.pubkey)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        if test {
            nodes.sort();
        }

        let cluster_size = nodes.len();
        // note the gossip_push_fanout * 3 is equivalent to CRDS_GOSSIP_PUSH_ACTIVE_SET_SIZE in solana
        // note, here the default is 6*3=18. but in solana it is 6*2=12
        self.active_set
            .rotate(rng, active_set_size, cluster_size, &nodes, stakes, self.pubkey());
    }

}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CrdsKey {
    origin: Pubkey,
    index: usize,
}

#[derive(Debug, Default)]
pub struct CrdsEntry {
    _ordinal: u64,
    _num_dups: u8,
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
    accounts: &HashMap<String, u64>,
    filter_zero_staked_nodes: bool,
) -> Result<Vec<(Node, Sender<Arc<Packet>>)>, Error> {
    info!("num of node pubkeys in vote accounts: {}", accounts.len());
    let now = Instant::now();

    // create node vector.
    // filter out 0 staked nodes if filter_zero_staked_nodes is true 
    let nodes: Vec<_> = accounts
        .into_iter()
        .filter(|(_, stake)| !filter_zero_staked_nodes || **stake != 0) 
        .map(|node| {
            let stake = accounts.get(node.0).copied().unwrap_or_default(); //get stake from 
                let pubkey = Pubkey::from_str(&node.0)?;
                let (sender, _receiver) = crossbeam_channel::unbounded();
                let node = Node {
                    _clock: now,
                    num_gossip_rounds: 0,
                    stake,
                    pubkey,
                    table: HashMap::default(),
                    active_set: PushActiveSet::default(),
                    received_cache: ReceivedCache::new(2 * CRDS_UNIQUE_PUBKEY_CAPACITY),
                    _receiver,
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
    accounts: &HashMap<String, u64>,
    filter_zero_staked_nodes: bool,
) -> Result<Vec<(Node, Sender<Arc<Packet>>)>, Error> {
    make_gossip_cluster(&accounts, filter_zero_staked_nodes)
}

#[allow(clippy::type_complexity)]
pub fn make_gossip_cluster_from_rpc(
    rpc_client: &RpcClient,
    filter_zero_staked_nodes: bool,
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
    // filter out zero-staked nodes if filter_zero_staked_nodes is true 
    let stakes: HashMap</*node pubkey:*/ String, /*activated stake:*/ u64> = vote_accounts
        .current
        .iter()
        .chain(&vote_accounts.delinquent)
        .into_grouping_map_by(|info| info.node_pubkey.clone())
        .aggregate(|stake, _node_pubkey, vote_account_info| {
            Some(stake.unwrap_or_default() + vote_account_info.activated_stake)
        })
        .into_iter()
        .filter(|(_, stake)| !filter_zero_staked_nodes || *stake != 0)
        .collect();
    
    make_gossip_cluster(&stakes, filter_zero_staked_nodes)
}

pub fn make_gossip_cluster_for_tests(
    accounts: &HashMap<Pubkey, u64>
) -> Result<Vec<(Node, Sender<Arc<Packet>>)>, Error> {
    info!("num of node pubkeys in vote accounts: {}", accounts.len());
    let now = Instant::now();

    let nodes: Vec<_> = accounts.into_iter().map(|node| {
        let stake = accounts.get(node.0).copied().unwrap_or_default(); //get stake from 
            let pubkey = node.0;
            let (sender, _receiver) = crossbeam_channel::unbounded();
            let node = Node {
                _clock: now,
                num_gossip_rounds: 0,
                stake,
                pubkey: *pubkey,
                table: HashMap::default(),
                active_set: PushActiveSet::default(),
                received_cache: ReceivedCache::new(2 * CRDS_UNIQUE_PUBKEY_CAPACITY),
                _receiver,
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

#[cfg(test)]
mod tests {
    use {
        super::*,
        rand::SeedableRng, rand_chacha::ChaChaRng, std::iter::repeat_with,
        rand::Rng,
        solana_sdk::{pubkey::Pubkey},
        std::{
            collections::{HashMap},
        },
        solana_sdk::native_token::LAMPORTS_PER_SOL,
    };

    pub fn get_bucket(stake: &u64) -> u64 {
        let stake = stake / LAMPORTS_PER_SOL;
        // say stake is high. few leading zeros. so bucket is high.
        let bucket = u64::BITS - stake.leading_zeros();
        // get min of 24 and bucket. higher numbered buckets are higher stake. low buckets low stake
        let bucket = (bucket as usize).min(25 - 1);
        bucket as u64
    }

    pub fn run_gossip(
        rng: &mut ChaChaRng,
        nodes: &mut Vec<Node>,
        stakes: &HashMap<Pubkey, /*stake:*/ u64>,
        active_set_size: usize,
    ) {
        for node in nodes {
            node.run_gossip(rng, stakes, active_set_size, true);
        }
    }

    #[test]
    fn test_mst() {
        const PUSH_FANOUT: usize = 2;
        const ACTIVE_SET_SIZE: usize = 12;
        let nodes: Vec<_> = repeat_with(Pubkey::new_unique).take(5).collect();
        const MAX_STAKE: u64 = (1 << 20) * LAMPORTS_PER_SOL;
        let mut rng = ChaChaRng::from_seed([189u8; 32]);
        let pubkey = Pubkey::new_unique();
        let stakes = repeat_with(|| rng.gen_range(1, MAX_STAKE));
        let mut stakes: HashMap<_, _> = nodes.iter().copied().zip(stakes).collect();
        stakes.insert(pubkey, rng.gen_range(1, MAX_STAKE));

        let res = make_gossip_cluster_for_tests(&stakes)
            .unwrap();

        let (mut nodes, _): (Vec<_>, Vec<_>) = res
            .into_iter()
            .map(|(node, sender)| {
                let pubkey = node.pubkey();
                (node, (pubkey, sender))
            })
            .unzip();

        // sort to maintain order across tests
        nodes.sort_by_key(|node| node.pubkey() );

        // setup gossip
        run_gossip(&mut rng, &mut nodes, &stakes, ACTIVE_SET_SIZE);
    
        let node_map: HashMap<Pubkey, &Node> = nodes
            .iter()
            .map(|node| (node.pubkey(), node))
            .collect();

        let mut cluster: Cluster = Cluster::new(PUSH_FANOUT);
        let origin_pubkey = &pubkey; //just a temp origin selection
        cluster.new_mst(origin_pubkey, &stakes, &node_map);

        // verify buckets
        let mut keys = stakes.keys().cloned().collect::<Vec<_>>();
        keys.sort_by_key(|&k| stakes.get(&k));

        let buckets: Vec<u64> = vec![15, 16, 19, 19, 20, 20];
        for (key, &bucket) in keys.iter().zip(buckets.iter()) {
            let current_stake = stakes.get(&key).unwrap();
            let bucket_from_stake = get_bucket(current_stake);
            assert_eq!(bucket, bucket_from_stake);
        }

        // in this test all nodes should be visited
        assert_eq!(cluster.get_visited_len(), 6);

        // check minimum distances
        assert_eq!(cluster.get_distance(&nodes[0].pubkey()), 2u64); // M, 2 hops from 5 -> M
        assert_eq!(cluster.get_distance(&nodes[1].pubkey()), 3u64); // h, 3 hops from 5 -> h
        assert_eq!(cluster.get_distance(&nodes[2].pubkey()), 1u64); // 3, 1 hop
        assert_eq!(cluster.get_distance(&nodes[3].pubkey()), 2u64); // P, 2 hops
        assert_eq!(cluster.get_distance(&nodes[4].pubkey()), 1u64); // j, 1 hop
        assert_eq!(cluster.get_distance(&nodes[5].pubkey()), 0u64); // 5, 0 hops

        // check number of inbound connections
        assert_eq!(cluster.get_num_inbound(&nodes[0].pubkey()), 3);
        assert_eq!(cluster.get_num_inbound(&nodes[1].pubkey()), 1);
        assert_eq!(cluster.get_num_inbound(&nodes[2].pubkey()), 3);
        assert_eq!(cluster.get_num_inbound(&nodes[3].pubkey()), 2);
        assert_eq!(cluster.get_num_inbound(&nodes[4].pubkey()), 3);
        
        // check num of hops per inbound connection
        // M
        assert_eq!(cluster.get_orders_hops(&nodes[0].pubkey(), &nodes[1].pubkey()).unwrap(), &4u64); // M <- h, 4 hops
        assert_eq!(cluster.get_orders_hops(&nodes[0].pubkey(), &nodes[4].pubkey()).unwrap(), &2u64); // M <- j, 2 hops

        // h
        assert_eq!(cluster.get_orders_hops(&nodes[1].pubkey(), &nodes[0].pubkey()).unwrap(), &3u64); // h <- M, 3 hops

        // 3 
        assert_eq!(cluster.get_orders_hops(&nodes[2].pubkey(), &nodes[0].pubkey()).unwrap(), &3u64); // 3 <- M, 3 hops
        assert_eq!(cluster.get_orders_hops(&nodes[2].pubkey(), &nodes[3].pubkey()).unwrap(), &3u64); // 3 <- P, 3 hops
        assert_eq!(cluster.get_orders_hops(&nodes[2].pubkey(), &nodes[5].pubkey()).unwrap(), &1u64); // 3 <- 5, 1 hop

        // P
        assert_eq!(cluster.get_orders_hops(&nodes[0].pubkey(), &nodes[1].pubkey()).unwrap(), &4u64); // M <- h, 4 hops
        assert_eq!(cluster.get_orders_hops(&nodes[0].pubkey(), &nodes[4].pubkey()).unwrap(), &2u64); // M <- j, 2 hops

        // j
        assert_eq!(cluster.get_orders_hops(&nodes[4].pubkey(), &nodes[2].pubkey()).unwrap(), &2u64); // j <- 3, 2 hops
        assert_eq!(cluster.get_orders_hops(&nodes[4].pubkey(), &nodes[3].pubkey()).unwrap(), &3u64); // j <- P, 3 hops
        assert_eq!(cluster.get_orders_hops(&nodes[4].pubkey(), &nodes[5].pubkey()).unwrap(), &1u64); // j <- 5, 1 hop

        // 5 - None
        // ensure origin is NOT in the orders map
        assert!(!cluster.get_order_key(&nodes[5].pubkey()));

        // test coverage. should be full coverage with 0 left out nodes
        assert_eq!(cluster.coverage(&stakes), (1f64, 0usize));

        // MST
        // 5 source
        assert_eq!(cluster.get_outbound_degree(&nodes[5].pubkey()), 2);
        assert_eq!(cluster.edge_exists(&nodes[5].pubkey(), &nodes[4].pubkey()), Ok(true)); // 5 -> j
        assert_eq!(cluster.edge_exists(&nodes[5].pubkey(), &nodes[2].pubkey()), Ok(true)); // 5 -> 3

        // j source
        assert_eq!(cluster.get_outbound_degree(&nodes[4].pubkey()), 2);
        assert_eq!(cluster.edge_exists(&nodes[4].pubkey(), &nodes[0].pubkey()), Ok(true)); // j -> M
        assert_eq!(cluster.edge_exists(&nodes[4].pubkey(), &nodes[3].pubkey()), Ok(true)); // j -> P

        // M source
        assert_eq!(cluster.get_outbound_degree(&nodes[0].pubkey()), 1);
        assert_eq!(cluster.edge_exists(&nodes[0].pubkey(), &nodes[1].pubkey()), Ok(true)); // M -> h

        // should never fail if above pass
        assert_eq!(cluster.edge_exists(&nodes[1].pubkey(), &nodes[0].pubkey()), Err(())); // h -> M should not exist (h not in map)
        assert_eq!(cluster.edge_exists(&nodes[3].pubkey(), &nodes[5].pubkey()), Err(())); // P -> 5 should not exist (P not in map)
        assert_eq!(cluster.edge_exists(&nodes[0].pubkey(), &nodes[4].pubkey()), Ok(false)); // M -> j should not exist
        assert_eq!(cluster.edge_exists(&nodes[4].pubkey(), &nodes[5].pubkey()), Ok(false)); // j -> 5 should not exist

        // m: 19, n: 6
        // 19 / (6 - 1) - 1 = 2.8
        assert_eq!(cluster.relative_message_redundancy(), Ok(2.8));

    }


    

}
