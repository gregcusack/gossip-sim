use {
    solana_gossip::weighted_shuffle::WeightedShuffle,
    indexmap::IndexMap,
    rand::Rng,
    solana_bloom::bloom::{AtomicBloom, Bloom},
    solana_sdk::{native_token::LAMPORTS_PER_SOL, pubkey::Pubkey},
    std::collections::HashMap,
    log::{debug},
};

const NUM_PUSH_ACTIVE_SET_ENTRIES: usize = 25;

// Each entry corresponds to a stake bucket for
//     min stake of { this node, crds value owner }
// The entry represents set of gossip nodes to actively
// push to for crds values belonging to the bucket.
// Each entry in the PushActiveSet array corresponds to a PushActiveSetEntry
// Each entry corresponds to a set of gossip nodes to actively push to for
// a crds value belonging to the bucket,. 
// Our active set is made up of 24 buckets. Each bucket contains an IndexMap<Pubkey, Bloom<Pubkey>>
// so when a node gets added to the active_set, it gets places in one of the buckets
// then when we have a CrdsValue to push, we get the bucket that the CrdsValue owner falls into
// and we return all the nodes that fall into that bucket
#[derive(Default)]
pub(crate) struct PushActiveSet([PushActiveSetEntry; NUM_PUSH_ACTIVE_SET_ENTRIES]);

// Keys are gossip nodes to push messages to.
// Values are which origins the node has pruned.
#[derive(Default)]
struct PushActiveSetEntry(IndexMap</*node:*/ Pubkey, /*origins:*/ AtomicBloom<Pubkey>>);

impl PushActiveSet {
    #[cfg(debug_assertions)]
    const MIN_NUM_BLOOM_ITEMS: usize = 512;
    #[cfg(not(debug_assertions))]
    const MIN_NUM_BLOOM_ITEMS: usize = crate::cluster_info::CRDS_UNIQUE_PUBKEY_CAPACITY;

    pub(crate) fn get_nodes<'a>(
        &'a self,
        pubkey: &Pubkey,    // This node.
        origin: &'a Pubkey, // CRDS value owner.
        // If true forces gossip push even if the node has pruned the origin.
        should_force_push: impl FnMut(&Pubkey) -> bool + 'a,
        stakes: &HashMap<Pubkey, u64>,
    ) -> impl Iterator<Item = &Pubkey> + 'a {
        // get the stake of the peer that originated this CrdsValue. 
        // compare that origin stake to our node's stake. Take whichever is smaller.
        let stake = stakes.get(pubkey).min(stakes.get(origin));
        // based on the stake above, get the PushActiveSetEntry at that bucket.
        // 
        self.get_entry(stake).get_nodes(origin, should_force_push)
    }

    // Prunes origins for the given gossip node.
    // We will stop pushing messages from the specified origins to the node.
    pub(crate) fn prune(
        &self,
        pubkey: &Pubkey,    // This node.
        node: &Pubkey,      // Gossip node.
        origins: &[Pubkey], // CRDS value owners.
        stakes: &HashMap<Pubkey, u64>,
    ) {
        let stake = stakes.get(pubkey);
        for origin in origins {
            if origin == pubkey {
                continue;
            }
            let stake = stake.min(stakes.get(origin));
            self.get_entry(stake).prune(node, origin)
        }
    }

    pub(crate) fn rotate<R: Rng>(
        &mut self,
        rng: &mut R,
        size: usize, // Number of nodes to retain in each active-set entry.
        cluster_size: usize,
        // Gossip nodes to be sampled for each push active set.
        nodes: &[Pubkey],
        stakes: &HashMap<Pubkey, u64>,
        self_pubkey: Pubkey,
    ) {
        let j = &self.0[0];
        // info!("greg pk: {:?}, PAS[0].len pre: {}", self_pubkey, j.0.len()); // all 0 at this point
        let num_bloom_filter_items = cluster_size.max(Self::MIN_NUM_BLOOM_ITEMS);
        // Active set of nodes to push to are sampled from these gossip nodes,
        // using sampling probabilities obtained from the stake bucket of each
        // node.
        //smaller buckets, smaller node stake. larger bucket, higher stake
        let buckets: Vec<_> = nodes
            .iter()
            .map(|node| get_stake_bucket(stakes.get(node)))
            .collect();
        // (k, entry) represents push active set where the stake bucket of
        //     min stake of {this node, crds value owner}
        // is equal to `k`. The `entry` maintains set of gossip nodes to
        // actively push to for crds values belonging to this bucket.
        let mut bucket_index: u64 = 0;
        for (k, entry) in self.0.iter_mut().enumerate() {
            // info!("greg pk: {:?}, PASE len: {}", self_pubkey, entry.0.len());
            let weights: Vec<u64> = buckets
                .iter()
                .map(|&bucket| {
                    // bucket <- get_stake_bucket(min stake of {
                    //  this node, crds value owner and gossip peer
                    // })
                    // weight <- (bucket + 1)^2
                    // min stake of {...} is a proxy for how much we care about
                    // the link, and tries to mirror similar logic on the
                    // receiving end when pruning incoming links:
                    // https://github.com/solana-labs/solana/blob/81394cf92/gossip/src/received_cache.rs#L100-L105
                    let bucket = bucket.min(k) as u64;
                    bucket.saturating_add(1).saturating_pow(2)
                })
                .collect();
            debug!("bucket index: {}", bucket_index);
            bucket_index += 1;
            entry.rotate(rng, size, num_bloom_filter_items, nodes, &weights, self_pubkey);
        }
        // let j = &self.0[0];
        // info!("greg pk: {:?}, PAS[0].len post: {}", self_pubkey, j.0.len()); //all 18 at this point
    }

    fn get_entry(&self, stake: Option<&u64>) -> &PushActiveSetEntry {
        let bucket = get_stake_bucket(stake);
        debug!("bucket index: {}", bucket);
        &self.0[bucket]
        // &self.0[get_stake_bucket(stake)]
    }
}

impl PushActiveSetEntry {
    const BLOOM_FALSE_RATE: f64 = 0.1;
    const BLOOM_MAX_BITS: usize = 1024 * 8 * 4;

    // return all nodes in the bucket. aka get all nodes in the PushActiveSetEntry
    // that corresponds to that bucket. (Bucket selected via stake of CrdsValue origin)
    // nodes to push to are placed in these buckets based on their stake
    fn get_nodes<'a>(
        &'a self,
        origin: &'a Pubkey, //origin of crdsvalue
        // If true forces gossip push even if the node has pruned the origin.
        mut should_force_push: impl FnMut(&Pubkey) -> bool + 'a,
    ) -> impl Iterator<Item = &Pubkey> + 'a {
        self.0
            .iter()
            .filter(move |(node, bloom_filter)| {
                !bloom_filter.contains(origin) || should_force_push(node)
            })
            .map(|(node, _bloom_filter)| node)
    }

    fn prune(
        &self,
        node: &Pubkey,   // Gossip node.
        origin: &Pubkey, // CRDS value owner
    ) {
        if let Some(bloom_filter) = self.0.get(node) {
            bloom_filter.add(origin);
        }
    }

    fn rotate<R: Rng>(
        &mut self,
        rng: &mut R,
        size: usize, // Number of nodes to retain.
        num_bloom_filter_items: usize,
        nodes: &[Pubkey],
        weights: &[u64],
        self_pubkey: Pubkey,
    ) {
        debug_assert_eq!(nodes.len(), weights.len());
        debug_assert!(weights.iter().all(|&weight| weight != 0u64));
        let shuffle = WeightedShuffle::new("rotate-active-set", weights).shuffle(rng);
        debug!("greg start rotate()");
        // info!("greg pk: {:?} weights, nodes, SAFE.len, size length: {}, {}, {}, {}", self_pubkey, weights.len(), nodes.len(), self.0.len(), size);
        for node in shuffle.map(|k| &nodes[k]) {
            // We intend to discard the oldest/first entry in the index-map.
            if self.0.len() > size {
                // info!("greg pk {} self.0.len > size, break!", self_pubkey);
                break;
            }
            if self.0.contains_key(node) {
                // info!("greg pk: {:?} self.0.contains node's pubkey. continue.", self_pubkey);
                continue;
            }
            let bloom = AtomicBloom::from(Bloom::random(
                num_bloom_filter_items,
                Self::BLOOM_FALSE_RATE,
                Self::BLOOM_MAX_BITS,
            ));
            debug!("greg pk: {:?} adding node to PASE: {:?}", self_pubkey, node);
            bloom.add(node);
            self.0.insert(*node, bloom);
        }
        debug!("greg end rotate()");
        // Drop the oldest entry while preserving the ordering of others.
        // info!("greg pk: {:?} out of loop, self.0.len() > size. remove nodes from SAFE. self.0.len(), size: {}, {}", self_pubkey, self.0.len(), size);
        while self.0.len() > size {
            self.0.shift_remove_index(0);
        }
    }
}

// Maps stake to bucket index.
fn get_stake_bucket(stake: Option<&u64>) -> usize {
    let stake = stake.copied().unwrap_or_default() / LAMPORTS_PER_SOL;
    // say stake is high. few leading zeros. so bucket is high.
    let bucket = u64::BITS - stake.leading_zeros();
    // get min of 24 and bucket. higher numbered buckets are higher stake. low buckets low stake
    (bucket as usize).min(NUM_PUSH_ACTIVE_SET_ENTRIES - 1)
}

#[cfg(test)]
mod tests {
    use {super::*, rand::SeedableRng, rand_chacha::ChaChaRng, std::iter::repeat_with};

    #[test]
    fn test_get_stake_bucket() {
        assert_eq!(get_stake_bucket(None), 0);
        let buckets = [0, 1, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 4, 5, 5];
        for (k, bucket) in buckets.into_iter().enumerate() {
            let stake = (k as u64) * LAMPORTS_PER_SOL;
            assert_eq!(get_stake_bucket(Some(&stake)), bucket);
        }
        for (stake, bucket) in [
            (4_194_303, 22),
            (4_194_304, 23),
            (8_388_607, 23),
            (8_388_608, 24),
        ] {
            let stake = stake * LAMPORTS_PER_SOL;
            assert_eq!(get_stake_bucket(Some(&stake)), bucket);
        }
        assert_eq!(
            get_stake_bucket(Some(&u64::MAX)),
            NUM_PUSH_ACTIVE_SET_ENTRIES - 1
        );
    }

    #[test]
    fn test_push_active_set() {
        const CLUSTER_SIZE: usize = 117;
        const MAX_STAKE: u64 = (1 << 20) * LAMPORTS_PER_SOL;
        let mut rng = ChaChaRng::from_seed([189u8; 32]);
        let pubkey = Pubkey::new_unique();
        let nodes: Vec<_> = repeat_with(Pubkey::new_unique).take(20).collect();
        let stakes = repeat_with(|| rng.gen_range(1, MAX_STAKE));
        let mut stakes: HashMap<_, _> = nodes.iter().copied().zip(stakes).collect();
        stakes.insert(pubkey, rng.gen_range(1, MAX_STAKE));
        let mut active_set = PushActiveSet::default();
        assert!(active_set.0.iter().all(|entry| entry.0.is_empty()));
        active_set.rotate(&mut rng, 5, CLUSTER_SIZE, &nodes, &stakes);
        assert!(active_set.0.iter().all(|entry| entry.0.len() == 5));
        // Assert that for all entries, each filter already prunes the key.
        for entry in &active_set.0 {
            for (node, filter) in entry.0.iter() {
                assert!(filter.contains(node));
            }
        }
        let other = &nodes[5];
        let origin = &nodes[17];
        assert!(active_set
            .get_nodes(&pubkey, origin, |_| false, &stakes)
            .eq([13, 5, 18, 16, 0].into_iter().map(|k| &nodes[k])));
        assert!(active_set
            .get_nodes(&pubkey, other, |_| false, &stakes)
            .eq([13, 18, 16, 0].into_iter().map(|k| &nodes[k])));
        active_set.prune(&pubkey, &nodes[5], &[*origin], &stakes);
        active_set.prune(&pubkey, &nodes[3], &[*origin], &stakes);
        active_set.prune(&pubkey, &nodes[16], &[*origin], &stakes);
        assert!(active_set
            .get_nodes(&pubkey, origin, |_| false, &stakes)
            .eq([13, 18, 0].into_iter().map(|k| &nodes[k])));
        assert!(active_set
            .get_nodes(&pubkey, other, |_| false, &stakes)
            .eq([13, 18, 16, 0].into_iter().map(|k| &nodes[k])));
        active_set.rotate(&mut rng, 7, CLUSTER_SIZE, &nodes, &stakes);
        assert!(active_set.0.iter().all(|entry| entry.0.len() == 7));
        assert!(active_set
            .get_nodes(&pubkey, origin, |_| false, &stakes)
            .eq([18, 0, 7, 15, 11].into_iter().map(|k| &nodes[k])));
        assert!(active_set
            .get_nodes(&pubkey, other, |_| false, &stakes)
            .eq([18, 16, 0, 7, 15, 11].into_iter().map(|k| &nodes[k])));
        let origins = [*origin, *other];
        active_set.prune(&pubkey, &nodes[18], &origins, &stakes);
        active_set.prune(&pubkey, &nodes[0], &origins, &stakes);
        active_set.prune(&pubkey, &nodes[15], &origins, &stakes);
        assert!(active_set
            .get_nodes(&pubkey, origin, |_| false, &stakes)
            .eq([7, 11].into_iter().map(|k| &nodes[k])));
        assert!(active_set
            .get_nodes(&pubkey, other, |_| false, &stakes)
            .eq([16, 7, 11].into_iter().map(|k| &nodes[k])));
    }

    #[test]
    fn test_push_active_set_entry() {
        const NUM_BLOOM_FILTER_ITEMS: usize = 100;
        let mut rng = ChaChaRng::from_seed([147u8; 32]);
        let nodes: Vec<_> = repeat_with(Pubkey::new_unique).take(20).collect();
        let weights: Vec<_> = repeat_with(|| rng.gen_range(1, 1000)).take(20).collect();
        let mut entry = PushActiveSetEntry::default();
        entry.rotate(
            &mut rng,
            5, // size
            NUM_BLOOM_FILTER_ITEMS,
            &nodes,
            &weights,
        );
        assert_eq!(entry.0.len(), 5);
        let keys = [&nodes[16], &nodes[11], &nodes[17], &nodes[14], &nodes[5]];
        assert!(entry.0.keys().eq(keys));
        for origin in &nodes {
            if !keys.contains(&origin) {
                assert!(entry.get_nodes(origin, |_| false).eq(keys));
            } else {
                assert!(entry.get_nodes(origin, |_| true).eq(keys));
                assert!(entry
                    .get_nodes(origin, |_| false)
                    .eq(keys.into_iter().filter(|&key| key != origin)));
            }
        }
        // Assert that each filter already prunes the key.
        for (node, filter) in entry.0.iter() {
            assert!(filter.contains(node));
        }
        for origin in keys {
            assert!(entry.get_nodes(origin, |_| true).eq(keys));
            assert!(entry
                .get_nodes(origin, |_| false)
                .eq(keys.into_iter().filter(|&node| node != origin)));
        }
        // Assert that prune excludes node from get.
        let origin = &nodes[3];
        entry.prune(&nodes[11], origin);
        entry.prune(&nodes[14], origin);
        entry.prune(&nodes[19], origin);
        assert!(entry.get_nodes(origin, |_| true).eq(keys));
        assert!(entry.get_nodes(origin, |_| false).eq(keys
            .into_iter()
            .filter(|&&node| node != nodes[11] && node != nodes[14])));
        // Assert that rotate adds new nodes.
        entry.rotate(&mut rng, 5, NUM_BLOOM_FILTER_ITEMS, &nodes, &weights);
        let keys = [&nodes[11], &nodes[17], &nodes[14], &nodes[5], &nodes[7]];
        assert!(entry.0.keys().eq(keys));
        entry.rotate(&mut rng, 6, NUM_BLOOM_FILTER_ITEMS, &nodes, &weights);
        let keys = [
            &nodes[17], &nodes[14], &nodes[5], &nodes[7], &nodes[1], &nodes[13],
        ];
        assert!(entry.0.keys().eq(keys));
        entry.rotate(&mut rng, 4, NUM_BLOOM_FILTER_ITEMS, &nodes, &weights);
        let keys = [&nodes[5], &nodes[7], &nodes[1], &nodes[13]];
        assert!(entry.0.keys().eq(keys));
    }
}
