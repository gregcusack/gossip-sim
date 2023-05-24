use {
    solana_client::client_error::ClientError,
    solana_sdk::pubkey::{ParsePubkeyError, Pubkey},
    std::fmt::Debug,
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

#[derive(Debug, Clone, Copy, Error, PartialEq)]
pub enum HopsStats {
    Mean(f64),
    Median(f64),
    Max(u64),
    Min(u64),
}

#[derive(Debug, Clone, Copy, Error, PartialEq)]
pub enum Stats {
    Mean(f64),
    Median(f64),
    Max(f64),
    Min(f64),
}

impl std::fmt::Display for HopsStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HopsStats::Mean(x) => write!(f, "Mean: {:.6}", x),
            HopsStats::Median(x) => write!(f, "Median: {:.2}", x),
            HopsStats::Max(x) => write!(f, "Max: {}", x),
            HopsStats::Min(x) => write!(f, "Min: {}", x),
        }
    }
}

impl std::fmt::Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Stats::Mean(x) => write!(f, "Mean: {:.6}", x),
            Stats::Median(x) => write!(f, "Median: {:.6}", x),
            Stats::Max(x) => write!(f, "Max: {:.6}", x),
            Stats::Min(x) => write!(f, "Min: {:.6}", x),
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
