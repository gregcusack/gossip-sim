use {
    crossbeam_channel::Sender,
    rand::Rng,
    solana_client::client_error::ClientError,
    solana_sdk::pubkey::{ParsePubkeyError, Pubkey},
    std::{collections::HashMap, fmt::Debug},
    thiserror::Error,
};

pub const API_MAINNET_BETA: &str = "https://api.mainnet-beta.solana.com";
pub const API_TESTNET: &str = "https://api.testnet.solana.com";

pub mod gossip_stats;
pub mod gossip;
mod push_active_set;
mod received_cache;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    ParsePubkeyError(#[from] ParsePubkeyError),
    #[error(transparent)]
    RouterError(#[from] RouterError),
    #[error("TryLockErrorPoisoned")]
    TryLockErrorPoisoned,
}

#[derive(Debug, Error)]
pub enum RouterError {
    #[error("duplicate pubkey: {0}")]
    DuplicatePubkey(Pubkey),
    #[error("invalid packet drop rate: {0}")]
    InvalidPacketDropRate(f64),
    #[error("node not found: {0}")]
    NodeNotFound(Pubkey),
    #[error("channel send error")]
    SendError,
}

#[derive(Debug, Error)]
pub enum HopsStats {
    Mean(f64),
    Median(f64),
    Max(u64),
    Min(u64),
}

#[derive(Debug, Error)]
pub enum CoverageStats {
    Mean(f64),
    Median(f64),
    Max(f64),
    Min(f64),
}



impl std::fmt::Display for HopsStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HopsStats::Mean(x) => write!(f, "Mean Hops: {:.6}", x),
            HopsStats::Median(x) => write!(f, "Median Hops: {:.2}", x),
            HopsStats::Max(x) => write!(f, "Max Hops: {}", x),
            HopsStats::Min(x) => write!(f, "Min Hops: {}", x),
        }
    }
}

impl std::fmt::Display for CoverageStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoverageStats::Mean(x) => write!(f, "Mean Coverage: {:.6}", x),
            CoverageStats::Median(x) => write!(f, "Median Coverage: {:.6}", x),
            CoverageStats::Max(x) => write!(f, "Max Coverage: {:.6}", x),
            CoverageStats::Min(x) => write!(f, "Min Coverage: {:.6}", x),
        }
    }
}

pub struct Router<T> {
    packet_drop_rate: f64,
    senders: HashMap<Pubkey, Sender<T>>,
}

impl<T> Router<T> {
    pub fn new<I>(packet_drop_rate: f64, nodes: I) -> Result<Self, RouterError>
    where
        I: IntoIterator<Item = (Pubkey, Sender<T>)>,
    {
        if !(0.0..=1.0).contains(&packet_drop_rate) {
            return Err(RouterError::InvalidPacketDropRate(packet_drop_rate));
        }
        let mut senders = HashMap::<Pubkey, Sender<T>>::new();
        for (pubkey, sender) in nodes {
            if senders.insert(pubkey, sender).is_some() {
                return Err(RouterError::DuplicatePubkey(pubkey));
            }
        }
        Ok(Self {
            packet_drop_rate,
            senders,
        })
    }
}

impl<T> Router<T> {
    fn send<R: Rng>(&self, rng: &mut R, node: &Pubkey, data: T) -> Result<(), RouterError> {
        // TODO: How to simulate packets arriving with delay?
        match self.senders.get(node) {
            None => Err(RouterError::NodeNotFound(*node)),
            Some(route) => {
                if rng.gen_bool(self.packet_drop_rate) {
                    Ok(()) // Silently drop packet
                } else {
                    route.send(data).map_err(|_| RouterError::SendError)
                }
            }
        }
    }
}

pub fn get_json_rpc_url(json_rpc_url: &str) -> &str {
    match json_rpc_url {
        "m" | "mainnet-beta" => API_MAINNET_BETA,
        "t" | "testnet" => API_TESTNET,
        _ => json_rpc_url,
    }
}
