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
        collections::{HashMap, HashSet},
        sync::Arc,
        time::{Instant},
        str::FromStr,
    },
};

pub(crate) const CRDS_UNIQUE_PUBKEY_CAPACITY: usize = 8192;


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
        // not the goassip_push_fanout * 3 is equivalent to CRDS_GOSSIP_PUSH_ACTIVE_SET_SIZE in solana
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