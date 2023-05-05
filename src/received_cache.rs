use {
    lru::LruCache,
    solana_sdk::pubkey::Pubkey,
    std::{collections::HashMap},
};

// For each origin, tracks which nodes have sent messages from that origin and
// their respective score in terms of timeliness of delivered messages.
pub(crate) struct ReceivedCache(LruCache</*origin/owner:*/ Pubkey, ReceivedCacheEntry>);

#[derive(Clone, Default)]
struct ReceivedCacheEntry {
    nodes: HashMap<Pubkey, /*score:*/ usize>,
    num_upserts: usize,
}

impl ReceivedCache {
    pub(crate) fn new(capacity: usize) -> Self {
        Self(LruCache::new(capacity))
    }

    #[cfg(test)]
    fn mock_clone(&self) -> Self {
        let mut cache = LruCache::new(self.0.cap());
        for (&origin, entry) in self.0.iter().rev() {
            cache.put(origin, entry.clone());
        }
        Self(cache)
    }
}