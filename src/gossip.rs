use solana_sdk::blake3::Hash;

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

#[cfg_attr(test, cfg(test))]
pub(crate) const CRDS_UNIQUE_PUBKEY_CAPACITY: usize = 8192;

pub struct Cluster {
    gossip_push_fanout: usize,

    // set of nodes that have allready been visited!
    visited: HashSet<Pubkey>,

    // keep track of the nodes that still need to be explored.
    queue: VecDeque<Pubkey>,

    // keep track of the shortest distance from the starting node to each node in the graph
    // dst_node: (hops, rx from)
    distances: HashMap<Pubkey, (u64, Pubkey)>,

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

    // stores all of the src nodes for a given dst node that is pruned
    // dst_pubkey => {src_pubkey, hops}
    prunes: HashMap<Pubkey, HashSet<Pubkey>>,


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

    pub fn get_orders(
        &self,
        dest_node: &Pubkey,
        src_node: &Pubkey,
    ) -> Option<&u64> {
        self.orders
            .get(dest_node)
            .unwrap()
            .get(src_node)
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
        self.distances.get(pubkey).unwrap().0
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
        for (pubkey, (hops, _)) in &self.distances {
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
        for (dest, prunes) in &self.prunes {
            info!("--------- Pruner: {:?} ---------", dest);
            for prune in prunes {
                info!("Prunee: {:?}", prune);
            }
        }
    }

    fn initialize(
        &mut self,
        stakes: &HashMap<Pubkey, u64>,
    ) {
        for (pubkey, _) in stakes {
            // Initialize the `distances` hashmap with a distance of infinity for each node in the graph
            self.distances.insert(*pubkey, (u64::MAX, Pubkey::default()));
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
        self.distances.insert(*origin_pubkey, (0, Pubkey::default())); //initialize beginning node
        self.queue.push_back(*origin_pubkey);
        self.visited.insert(*origin_pubkey);

        // going through BFS
        while !self.queue.is_empty() {
            // Dequeue the next node from the queue and get its current distance
            let current_node_pubkey = self.queue.pop_front().unwrap();
            let (current_distance, _) = self.distances[&current_node_pubkey];

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
                .take(self.gossip_push_fanout) {
                    debug!("curr node, neighbor: {:?}, {:?}", current_node.pubkey(), neighbor);
                    //check if we have visited this node before.
                    if !self.visited.contains(neighbor) {
                        // if not, we insert it
                        self.visited.insert(*neighbor);
                        //update the distance. saying the neighbor node we are looking at is
                        // an additional hop from the current node. so it is going to be 
                        // an additional hop
                        self.distances.insert(*neighbor, (current_distance + 1, current_node_pubkey));
                        
                        self.queue.push_back(*neighbor);

                        // found a new neighbor for our current node
                        // add the new neighbor to the current node's adjacency hashset
                        self.mst
                                .entry(current_node_pubkey)
                                .or_insert_with(|| HashSet::<Pubkey>::new())
                                .insert(*neighbor);
                    } else { //  have seen neighbor, so compare hops to get to neighbor from what 
                             //  we've previously seen
                        // get min number of hops to neighbor that we know of
                        let (minimum_known_dist, from) = self.distances
                            .get(neighbor)
                            .unwrap()
                            .clone(); 

                        if current_distance + 1 < minimum_known_dist {
                            // we have found a faster way (fewer hops) to get to the neighbor node
                            // update distances to have shorter distance to neighbor 
                            self.distances
                                .insert(*neighbor, (current_distance + 1, current_node_pubkey));

                            // remove the old from mst.
                            // fewest hops used to be from "from" => neighbor.
                            // now it is "current_node" => "neighbor"
                            self.mst
                                .get_mut(&from)
                                .unwrap()
                                .remove(neighbor);

                            self.mst
                                .entry(current_node_pubkey)
                                .or_insert_with(|| HashSet::<Pubkey>::new())
                                .insert(*neighbor);


                        } else {
                            // since we already know of a quicker way to get to this neighbor
                            // add the current node to the neighbor's prune list.
                            // neighbor pruning current node
                            self.prunes
                                .entry(*neighbor)
                                .or_insert_with(|| HashSet::<Pubkey>::new())
                                .insert(current_node_pubkey);
                        }

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

    // pub fn generate_prunes(
    //     &mut self,
    // ) {
        // for (dst, src_hops_map) in self.orders.iter() {
        //     let mut prunes_vec: Vec<(Pubkey, u64)> = Vec::new();
        //     let mut mst_src: Option<&Pubkey> = None;
        //     let mut min_hops: Option<&u64> = None;
        
        //     for (src, hops) in src_hops_map.iter() {
        //         if min_hops.is_none() || *hops < *min_hops.unwrap() {
        //             if let Some(old_mst_src) = self.mst.insert(*dst, (*src, *hops)) {
        //                 prunes_vec.push(old_mst_src);
        //             }
        //             min_hops = Some(hops);
        //             mst_src = Some(src);
        //         } else {
        //             prunes_vec.push((*src, *hops));
        //         }
        //     }
        //     if let Some(mst_src) = mst_src {
        //         self.mst.insert(*dst, (*mst_src, *min_hops.unwrap()));
        //     }
        //     if !prunes_vec.is_empty() {
        //         self.prunes.insert(*dst, prunes_vec);
        //     }
        // }

        // info!("MST: ");
        // for (dst, (src, hops)) in &self.mst {
        //     info!("dst, src, hops: {:?}, {:?}, {}", dst, src, hops);
        // }

        // for (dst, pruned) in &self.prunes {
        //     info!("---------- Pruner: {:?} ----------", dst);
        //     for (prune, hops) in pruned {
        //         info!("Prunee src, hops: {:?}, {}", prune, hops);
        //     }
        // }

        // for (dst, src_hops) in &self.orders {
        //     let len_prunes = self.prunes.get(dst).unwrap().len();
        //     assert_eq!(src_hops.len() - 1, len_prunes);
        // }


    // }
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
        let mut nodes: Vec<_> = stakes
            .keys()
            .copied()
            .chain(self.table.keys().map(|key| key.origin))
            .filter(|pubkey| pubkey != &self.pubkey)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        if cfg!(test) {
            nodes.sort();
        }

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

pub fn make_gossip_cluster_for_tests(
    accounts: &HashMap<Pubkey, u64>
) -> Result<Vec<(Node, Sender<Arc<Packet>>)>, Error> {
    info!("num of node pubkeys in vote accounts: {}", accounts.len());
    let now = Instant::now();

    let nodes: Vec<_> = accounts.into_iter().map(|node| {
        let stake = accounts.get(node.0).copied().unwrap_or_default(); //get stake from 
            let pubkey = node.0;
            let (sender, receiver) = crossbeam_channel::unbounded();
            let node = Node {
                clock: now,
                num_gossip_rounds: 0,
                stake,
                pubkey: *pubkey,
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
    ) {
        // let mut rng = rand::thread_rng();
        for node in nodes {
            node.run_gossip(rng, stakes);
        }
    }

    #[test]
    fn test_mst() {
        const PUSH_FANOUT: usize = 2;
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
        run_gossip(&mut rng, &mut nodes, &stakes);
    
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
        assert_eq!(cluster.get_orders(&nodes[0].pubkey(), &nodes[1].pubkey()).unwrap(), &4u64); // M <- h, 4 hops
        assert_eq!(cluster.get_orders(&nodes[0].pubkey(), &nodes[4].pubkey()).unwrap(), &2u64); // M <- j, 2 hops

        // h
        assert_eq!(cluster.get_orders(&nodes[1].pubkey(), &nodes[0].pubkey()).unwrap(), &3u64); // h <- M, 3 hops

        // 3 
        assert_eq!(cluster.get_orders(&nodes[2].pubkey(), &nodes[0].pubkey()).unwrap(), &3u64); // 3 <- M, 3 hops
        assert_eq!(cluster.get_orders(&nodes[2].pubkey(), &nodes[3].pubkey()).unwrap(), &3u64); // 3 <- P, 3 hops
        assert_eq!(cluster.get_orders(&nodes[2].pubkey(), &nodes[5].pubkey()).unwrap(), &1u64); // 3 <- 5, 1 hop

        // P
        assert_eq!(cluster.get_orders(&nodes[0].pubkey(), &nodes[1].pubkey()).unwrap(), &4u64); // M <- h, 4 hops
        assert_eq!(cluster.get_orders(&nodes[0].pubkey(), &nodes[4].pubkey()).unwrap(), &2u64); // M <- j, 2 hops

        // j
        assert_eq!(cluster.get_orders(&nodes[4].pubkey(), &nodes[2].pubkey()).unwrap(), &2u64); // j <- 3, 2 hops
        assert_eq!(cluster.get_orders(&nodes[4].pubkey(), &nodes[3].pubkey()).unwrap(), &3u64); // j <- P, 3 hops
        assert_eq!(cluster.get_orders(&nodes[4].pubkey(), &nodes[5].pubkey()).unwrap(), &1u64); // j <- 5, 1 hop

        // 5 - None
        // ensure origin is NOT in the orders map
        assert!(!cluster.get_order_key(&nodes[5].pubkey()));

        // test coverage. should be full coverage with 0 left out nodes
        assert_eq!(cluster.coverage(&stakes), (1f64, 0usize));

    }

}
