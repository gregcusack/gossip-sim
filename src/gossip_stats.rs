use {
    crate::{Stats, HopsStats},
    log::{info, error, warn, debug},
    std::collections::{HashMap, BTreeMap, HashSet},
    solana_sdk::pubkey::Pubkey,
    crate::gossip::{Testing, StepSize, Config},
};

// stores stats about a single run of mst. 
#[derive(Debug, Clone)]
pub struct HopsStat {
    mean: HopsStats,
    median: HopsStats,
    max: HopsStats,
    min: HopsStats,
}

impl Default for HopsStat {
    fn default() -> Self {
        HopsStat {
            mean: HopsStats::Mean(0.0),
            median: HopsStats::Median(0.0),
            max: HopsStats::Max(0),
            min: HopsStats::Min(0),
        }
    }
}

impl HopsStat {
    pub fn new(
        // distances: &HashMap<Pubkey, u64>
        hops: &mut Vec<u64>,
    ) -> HopsStat {
        // let mut hops: Vec<u64> = hops.values().cloned().collect();
        hops.sort();

        // filter out nodes not visited (u64::MAX)
        // filter out the origin node hops which is always 0.
        let hops: Vec<u64> = hops
            .iter()
            .filter(|&v| *v != u64::MAX && *v != 0)
            .cloned()
            .collect();
        let count = hops.len();

        // Calculate the mean filter out nodes that haven't been visited and have a dist of u64::MAX
        let mean = hops
            .iter()
            .sum::<u64>() as f64 / count as f64;

        // Calculate the median
        let median = if count % 2 == 0 {
            let mid = count / 2;

            (hops[mid - 1] + hops[mid]) as f64 / 2.0
        } else {
            hops[count / 2] as f64
        };

        // Calculate the min and max
        let max = *hops
            .iter()
            .filter(|&v| *v != u64::MAX)
            .last()
            .unwrap_or(&0);
        
        let min = *hops
            .first()
            .unwrap_or(&0);

        HopsStat { 
            mean: HopsStats::Mean(mean), 
            median: HopsStats::Median(median), 
            max: HopsStats::Max(max),
            min: HopsStats::Min(min), 
        }

    }

    pub fn mean(&self) -> f64 {
        match &self.mean {
            HopsStats::Mean(val) => *val,
            _ => panic!("Unexpected value in mean field"),
        }
    }

    pub fn max(&self) -> u64 {
        match &self.max {
            HopsStats::Max(val) => *val,
            _ => panic!("Unexpected value in max field"),
        }
    }

    pub fn min(&self) -> u64 {
        match &self.min {
            HopsStats::Min(val) => *val,
            _ => panic!("Unexpected value in min field"),
        }
    }

    pub fn median(&self) -> f64 {
        match &self.median {
            HopsStats::Median(val) => *val,
            _ => panic!("Unexpected value in median field"),
        }
    }

    pub fn print_stats (
        &self,
    ) {
        info!("Hops {}", self.mean);
        info!("Hops {}", self.median);
        info!("Hops {}", self.max);
        // info!("Hops {}", self.min); // min hops is always 0
    }
    
}

// if we run multiple MSTs, this will keep track of the hops
// over the course of multiple runs.
#[derive(Debug, Clone)]
pub struct HopsStatCollection {
    per_round_stats: Vec<HopsStat>,
    raw_hop_collection: Vec<u64>, // all hop counts seen
    aggregate_stats: HopsStat,
    last_delivery_hop_stats: HopsStat,
    histogram: Histogram,
}

impl Default for HopsStatCollection {
    fn default() -> Self {
        Self {
            per_round_stats: Vec::default(),
            raw_hop_collection: Vec::default(),
            aggregate_stats: HopsStat::default(),
            last_delivery_hop_stats: HopsStat::default(),
            histogram: Histogram::default(),
        }
    }
}

impl HopsStatCollection {
    pub fn insert(
        &mut self,
        // distances: &HashMap<Pubkey, u64>,
        hops: &mut Vec<u64>,
    ) {
        self.per_round_stats.push(HopsStat::new(hops));
        
        for hops in hops
            .iter()
            .filter(|hops| *hops != &u64::MAX) {
                self.raw_hop_collection.push(*hops);
        }
    }

    pub fn get_stat_by_iteration(
        &mut self,
        index: usize,
    ) -> &HopsStat {
        &self.per_round_stats[index]
    }

    pub fn aggregate_hop_stats(
        &mut self,
    ) {
        self.aggregate_stats = HopsStat::new(&mut self.raw_hop_collection);
    }

    pub fn get_aggregate_hop_stats(
        &self,
    ) -> &HopsStat {
        &self.aggregate_stats
    }

    pub fn calc_last_delivery_hop_stats(
        &mut self,
    ) {
        let mut vec: Vec<u64> = Vec::new();
        for hopstat in self.per_round_stats.iter() {
            vec.push(hopstat.max());
        }
        self.last_delivery_hop_stats = HopsStat::new(&mut vec);
    }

    pub fn get_last_delivery_hop_stats(
        &self,
    ) -> &HopsStat {
        &self.last_delivery_hop_stats
    }

    //TODO: turn this into its own object that is held by the stranded stats collection
    pub fn build_histogram(
        &mut self,
        num_buckets: u64,
    ) {
        self.histogram.build(num_buckets, &self.raw_hop_collection);
    }

    pub fn get_histogram(
        &self,
    ) -> &Histogram {
        &self.histogram
    }

}

#[derive(Debug, Clone)]
pub struct StatCollection {
    collection: Vec<f64>,
    mean: Stats,
    median: Stats,
    max: Stats,
    min: Stats,
    collection_type: String,
}

impl Default for StatCollection {
    fn default() -> Self {
        Self {
            collection: Vec::default(),
            mean: Stats::Mean(0.0),
            median: Stats::Median(0.0),
            max: Stats::Max(0.0),
            min: Stats::Min(0.0),
            collection_type: String::new(),
        }
    }
}

impl StatCollection {
    pub fn new(
        collection_type: &str,
    ) -> Self {
        StatCollection {
            collection: Vec::default(),
            mean: Stats::Mean(0.0),
            median: Stats::Median(0.0),
            max: Stats::Max(0.0),
            min: Stats::Min(0.0),
            collection_type: String::from(collection_type),
        }
    }

    pub fn calculate_stats (
        &mut self,
    ) {
        // clone to maintain iteration order for print_stats
        let mut sorted_coverages = self.collection.clone();
        sorted_coverages
            .sort_by(|a, b| a
                    .partial_cmp(b)
                    .unwrap());
        let len = sorted_coverages.len();
        let mean = sorted_coverages
            .iter()
            .sum::<f64>() / len as f64;
        let median = if len % 2 == 0 {
            (sorted_coverages[len / 2 - 1] + sorted_coverages[len / 2]) / 2.0
        } else {
            sorted_coverages[len / 2]
        };
        let max = *sorted_coverages
            .last()
            .unwrap_or(&0.0);
        let min = *sorted_coverages
            .first()
            .unwrap_or(&0.0);

        self.mean = Stats::Mean(mean);
        self.median = Stats::Median(median);
        self.max = Stats::Max(max);
        self.min = Stats::Min(min);
    }

    pub fn mean(&self) -> f64 {
        match &self.mean {
            Stats::Mean(val) => *val,
            _ => panic!("Unexpected value in mean field"),
        }
    }

    pub fn max(&self) -> f64 {
        match &self.max {
            Stats::Max(val) => *val,
            _ => panic!("Unexpected value in max field"),
        }
    }

    pub fn min(&self) -> f64 {
        match &self.min {
            Stats::Min(val) => *val,
            _ => panic!("Unexpected value in min field"),
        }
    }

    pub fn median(&self) -> f64 {
        match &self.median {
            Stats::Median(val) => *val,
            _ => panic!("Unexpected value in median field"),
        }
    }

    pub fn get_stat_by_index(
        &self,
        index: usize,
    ) -> f64 {
        self.collection[index]
    }

    pub fn print_stats (
        &self,
    ) {
        // info!("Number of iterations: {}", self.coverages.len());
        info!("{} {}", self.collection_type, self.mean);
        info!("{} {}", self.collection_type, self.median);
        info!("{} {}", self.collection_type, self.max);
        info!("{} {}", self.collection_type, self.min);
    }    
}

// RMR = m / (n - 1) - 1
// m: total number of payload messages exchanged during gossip (push/prune)
// n: total number of nodes that receive the message
#[derive(Debug, Clone)]
pub struct RelativeMessageRedundancy {
    m: u64,
    n: u64,
    rmr: f64,
}

impl Default for RelativeMessageRedundancy {
    fn default() -> Self {
        RelativeMessageRedundancy { 
            m: 0,
            n: 0,
            rmr: 0.0,
        }
    }
}

impl RelativeMessageRedundancy {
    pub fn increment_m(
        &mut self,
    ) {
        self.m += 1;
    }

    pub fn increment_n(
        &mut self,
    ) {
        self.n += 1;
    }

    pub fn reset(
        &mut self,
    ) {
        self.m = 0;
        self.n = 0;
        self.rmr = 0.0;
    }

    pub fn calculate_rmr(
        &mut self,
    ) -> Result<f64, String> {
        if self.n == 0 {
            Err("Division by zero. n is 0.".to_string())
        } else {
            self.rmr = self.m as f64 / (self.n - 1) as f64 - 1.0;
            Ok(self.rmr)
        }
    }

    pub fn rmr(
        &self,
    ) -> f64 {
        self.rmr
    }

}

impl std::fmt::Display for RelativeMessageRedundancy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "m: {}, n: {}, rmr: {:.6}", self.m, self.n, self.rmr)
    }
}

#[derive(Debug, Clone)]
pub struct Histogram {
    // histogram. buckets are stranded count.
    // amount in bucket is the number of nodes that were stranded that many times
    entries: BTreeMap<u64, u64>,

    min_entry: u64,
    max_entry: u64,
    bucket_range: u64,
    num_buckets: u64,

}

impl Default for Histogram {
    fn default() -> Self {
        Self {
            entries: BTreeMap::default(),
            min_entry: 0,
            max_entry: 0,
            bucket_range: 0,
            num_buckets: 0,
        }
    }
}

impl Histogram {
    //TODO: turn this into its own object that is held by the stranded stats collection
    pub fn build(
        &mut self,
        num_buckets: u64,
        input_entries: &Vec<u64>,
    ) {        
        self.set_num_buckets(num_buckets);
        // Determine the range of each bucket
        self.set_min_entry(
            input_entries
                    .iter()
                    .map(|entry| *entry)
                    .min()
                    .unwrap_or(0));

        self.set_max_entry(
            input_entries
                    .iter()
                    .map(|entry| *entry)
                    .max()
                    .unwrap_or(0));


        if self.max_entry == self.min_entry {
            warn!("WARNING: Max and Min histogram entries are the same.");
            self.set_bucket_range(1);
        } else {
            self.set_bucket_range(
                (self.max_entry - self.min_entry + self.num_buckets - 1) / self.num_buckets);    
        }

        // Initialize all buckets with 0 entries
        for bucket in 0..self.num_buckets {
            self.entries.insert(bucket, 0);
        }
 
        // Iterate over the stranded_nodes entries
        for hops in input_entries.iter() {
            // Determine the bucket index based on the times_stranded value
            let bucket = (*hops - self.min_entry) / self.bucket_range;
            
            // Increment the count for the bucket in the histogram
            *self.entries.entry(bucket).or_insert(0) += 1;
        }
    }

    pub fn set_min_entry(
        &mut self,
        val: u64,
    ) {
        self.min_entry = val;
    }

    pub fn set_max_entry(
        &mut self,
        val: u64,
    ) {
        self.max_entry = val;
    }

    pub fn set_bucket_range(
        &mut self,
        val: u64,
    ) {
        self.bucket_range = val;
    }

    pub fn min_entry(
        &self,
    ) -> u64 {
        self.min_entry
    }

    pub fn max_entry(
        &self,
    ) -> u64 {
        self.max_entry
    }

    pub fn bucket_range(
        &self,
    ) -> u64 {
        self.bucket_range
    }

    pub fn set_num_buckets(
        &mut self,
        val: u64,
    ) {
        self.num_buckets = val;
    }

    pub fn num_buckets(
        &self,
    ) -> u64 {
        self.num_buckets
    }


}

#[derive(Debug)]
pub struct StrandedNodeCollection {
    stranded_nodes: HashMap<Pubkey, (/* stake */u64, /* times stranded */ u64)>, 
    /*
    TODO: histogram -> # of nodes stranded for n iterations
     */
    total_gossip_iterations: u64,
    total_stranded_iterations: u64, // sum(times_stranded)
    mean_stranded_per_iteration: f64, // sum(times_stranded) / iterations

    // across just the stranded nodes, what is the mean number of iterations
    mean_standed_iterations_per_stranded_node: f64, // mean # of iterations a stranded node was stranded for

    // across just the stranded nodes, what is the median number of iterations you will be stranded for
    median_standed_iterations_per_stranded_node: f64, // median(times_stranded)

    // across all nodes in the network. the average amount of iterations a node was stranded for
    stranded_iterations_per_node: f64,  // total_stranded_iterations / # of nodes in network

    total_nodes: usize, // for stranded_iterations_per_node

    // Info about the stake of stranded nodes
    total_stranded_stake: u64,
    stranded_node_mean_stake: f64,
    stranded_node_median_stake: f64,
    stranded_node_max_stake: u64,
    stranded_node_min_stake: u64,

    // Info about the stake of stranded nodes
    // For mean and median stake of stranded nodes, 
    // each time a node is stranded it’s stake should be added to the total stake stranded. 
    // So if a node with stake 100 is stranded 50 times. 100 should be added into the stranded stake 50 times. 
    // ensures that a highly staked node only stranded once doesn't affect the mean/median as equally as 
    // a low staked node stranded 100 times
    weighted_total_stranded_stake: u64,
    weighted_stranded_node_mean_stake: f64,
    weighted_stranded_node_median_stake: f64,

    // // histogram. buckets are stranded count.
    // // amount in bucket is the number of nodes that were stranded that many times
    histogram: Histogram,
}

impl Default for StrandedNodeCollection {
    fn default() -> Self {
        Self {
            stranded_nodes: HashMap::default(),
            total_gossip_iterations: 0,
            total_stranded_iterations: 0,
            mean_stranded_per_iteration: 0.0,
            mean_standed_iterations_per_stranded_node: 0.0,
            median_standed_iterations_per_stranded_node: 0.0,
            stranded_iterations_per_node: 0.0,
            total_nodes: 0,
            total_stranded_stake: 0,
            stranded_node_mean_stake: 0.0,
            stranded_node_median_stake: 0.0,
            stranded_node_max_stake: 0,
            stranded_node_min_stake: 0,  
            weighted_total_stranded_stake: 0,
            weighted_stranded_node_mean_stake: 0.0,
            weighted_stranded_node_median_stake: 0.0, 
            histogram: Histogram::default(),    
        }
    }
}

impl Clone for StrandedNodeCollection {
    fn clone(&self) -> Self {
        StrandedNodeCollection {
            stranded_nodes: self.stranded_nodes.clone(),
            total_gossip_iterations: self.total_gossip_iterations,
            total_stranded_iterations: self.total_stranded_iterations,
            mean_stranded_per_iteration: self.mean_stranded_per_iteration,
            mean_standed_iterations_per_stranded_node: self.mean_standed_iterations_per_stranded_node,
            median_standed_iterations_per_stranded_node: self.median_standed_iterations_per_stranded_node,
            stranded_iterations_per_node: self.stranded_iterations_per_node,
            total_nodes: self.total_nodes,
            total_stranded_stake: self.total_stranded_stake,
            stranded_node_mean_stake: self.stranded_node_mean_stake,
            stranded_node_median_stake: self.stranded_node_median_stake,
            stranded_node_max_stake: self.stranded_node_max_stake,
            stranded_node_min_stake: self.stranded_node_min_stake,

            weighted_total_stranded_stake: self.weighted_total_stranded_stake,
            weighted_stranded_node_mean_stake: self.weighted_stranded_node_mean_stake,
            weighted_stranded_node_median_stake: self.weighted_stranded_node_median_stake,
            histogram: self.histogram.clone(),
        }
    }
}

// impl Copy for StrandedNodeCollection {}

impl StrandedNodeCollection {
    fn increment_stranded_count(
        &mut self,
        pubkey: &Pubkey,
        stakes: &HashMap<Pubkey, u64>,
    ) {
        // Check if the pubkey has already been stranded
        if let Some((_, count)) = self.stranded_nodes.get_mut(pubkey) {
            // Increment the number of times its been stranded
            *count += 1;
        } else {
            // if pubkey has not been stranded before, add a new entry using stake from stakes map
            if let Some(&stake) = stakes.get(pubkey) {
                self.stranded_nodes.insert(*pubkey, (stake, 1));
            } else {
                // We should never get here. stakes should hold all values in map
                error!("Stake for pubkey {:?} not found", pubkey);
            }
        }
    }

    pub fn calculate_stats(
        &mut self,
    ) {
        self.total_stranded_iterations = 0;
        self.total_stranded_stake = 0;
        self.weighted_total_stranded_stake = 0;
        let mut stranded_iteration_counts: Vec<u64> = Vec::new();
        let mut stranded_stakes: Vec<u64> = Vec::new();
        let mut weighted_stranded_stakes: Vec<u64> = Vec::new();

        for (_, (stake, times_stranded)) in self.stranded_nodes.iter() {
            self.total_stranded_iterations += times_stranded;
            stranded_iteration_counts.push(*times_stranded);

            self.total_stranded_stake += stake;
            self.weighted_total_stranded_stake += stake * times_stranded;
            stranded_stakes.push(*stake);
            for _ in 0..*times_stranded {
                weighted_stranded_stakes.push(*stake);
            }
            
        }

        self.mean_stranded_per_iteration = self.total_stranded_iterations as f64 / self.total_gossip_iterations as f64;
        self.stranded_node_mean_stake = self.total_stranded_stake as f64 / self.stranded_count() as f64;
        self.mean_standed_iterations_per_stranded_node = self.total_stranded_iterations as f64 / self.stranded_count() as f64;


        self.weighted_stranded_node_mean_stake = self.weighted_total_stranded_stake as f64 / self.total_stranded_iterations as f64;

        // info!("stranded count, total gossip iters: {}, {}", self.stranded_count(), self.total_gossip_iterations);

        stranded_iteration_counts.sort();
        stranded_stakes.sort();
        weighted_stranded_stakes.sort();

        self.median_standed_iterations_per_stranded_node = if stranded_iteration_counts.is_empty() {
            0.0
        } else if stranded_iteration_counts.len() % 2 == 0 {
            let mid = stranded_iteration_counts.len() / 2;
            (stranded_iteration_counts[mid - 1] + stranded_iteration_counts[mid]) as f64 / 2.0
        } else {
            stranded_iteration_counts[stranded_iteration_counts.len() / 2] as f64
        };

        // info!("stranded iter, total nodes: {}, {}", self.total_stranded_iterations, self.total_nodes);
        self.stranded_iterations_per_node = self.total_stranded_iterations as f64 / self.total_nodes as f64;

        self.stranded_node_median_stake = if stranded_stakes.is_empty() {
            0.0
        } else if stranded_stakes.len() % 2 == 0 {
            let mid = stranded_stakes.len() / 2;
            (stranded_stakes[mid - 1] + stranded_stakes[mid]) as f64 / 2.0
        } else {
            stranded_stakes[stranded_stakes.len() / 2] as f64
        };

        self.weighted_stranded_node_median_stake = if weighted_stranded_stakes.is_empty() {
            0.0
        } else if weighted_stranded_stakes.len() % 2 == 0 {
            let mid = weighted_stranded_stakes.len() / 2;
            (weighted_stranded_stakes[mid - 1] + weighted_stranded_stakes[mid]) as f64 / 2.0
        } else {
            weighted_stranded_stakes[weighted_stranded_stakes.len() / 2] as f64
        };


        self.stranded_node_max_stake = *stranded_stakes
            .last()
            .unwrap_or(&0);
        self.stranded_node_min_stake = *stranded_stakes
            .first()
            .unwrap_or(&0);

    }

    pub fn insert_nodes(
        &mut self,
        stranded_nodes: &Vec<Pubkey>,
        stakes: &HashMap<Pubkey, u64>,
    ) {
        for pubkey in stranded_nodes.iter() {
            self.increment_stranded_count(pubkey, stakes);
        }
        // we only call this method once per gossip iteration
        self.total_gossip_iterations += 1;
        // set for stranded_iterations_per_node calculation later
        if self.total_nodes == 0 {
            self.total_nodes = stakes.len();
        }
    }

    pub fn get_stranded_iterations_per_node(
        &self,
    ) -> f64 {
        self.stranded_iterations_per_node
    }

    pub fn get_sorted_stranded(
        &self,
    ) -> Vec<(Pubkey, (u64, u64))> {
        let mut sorted_nodes: Vec<(Pubkey, (u64, u64))> = self.stranded_nodes
            .clone()
            .into_iter()
            .collect();
        sorted_nodes.sort_by(|(_, (stake1, times_stranded1)), (_, (stake2, times_stranded2))| {
            match times_stranded1.cmp(times_stranded2).reverse() {
                std::cmp::Ordering::Equal => stake1.cmp(stake2).reverse(),
                other => other,
            }
        });
        sorted_nodes
    }

    pub fn stranded_count(
        &self,
    ) -> usize {
        self.stranded_nodes.len()
    }

    pub fn get_total_stranded_iterations(
        &self,
    ) -> u64 {
        self.total_stranded_iterations
    }

    pub fn get_mean_stranded_per_iteration(
        &self,
    ) -> f64 {
        self.mean_stranded_per_iteration
    }

    pub fn get_median_standed_iterations_per_stranded_node(
        &self,
    ) -> f64 {
        self.median_standed_iterations_per_stranded_node
    }

    pub fn get_mean_standed_iterations_per_stranded_node(
        &self,
    ) -> f64 {
        self.mean_standed_iterations_per_stranded_node
    }

    pub fn get_stranded_stake_stats(
        &self,
    ) -> (f64, f64, u64, u64) {
        (
            self.stranded_node_mean_stake, 
            self.stranded_node_median_stake, 
            self.stranded_node_max_stake, 
            self.stranded_node_min_stake
        )
    }

    pub fn get_weighted_stranded_stake_stats(
        &self,
    ) -> (f64, f64) {
        (
            self.weighted_stranded_node_mean_stake, 
            self.weighted_stranded_node_median_stake, 
        )
    }

    //TODO: turn this into its own object that is held by the stranded stats collection
    pub fn build_histogram(
        &mut self,
        num_buckets: u64,
    ) {        
        self.histogram.build(
            num_buckets, 
            &self.stranded_nodes
                .values()
                .map(|&(_, value)| value)
                .collect()
            );
    }

    pub fn get_histogram(
        &self,
    ) -> &Histogram {
        &self.histogram
    }

}

#[derive(Debug, Clone)]
pub struct BranchingFactor { }

// NOTE: This will measure branching factor of all visited nodes
// Does NOT include nodes that were not visited
impl BranchingFactor {
    pub fn calculate_outbound(
        // &mut self,
        pushes: &HashMap<Pubkey, HashSet<Pubkey>>,
    ) -> f64 {
        let total_nodes = pushes.len();
        let mut outgoing_connections = 0;
    
        for destination in pushes.values() {
            outgoing_connections += destination.len();
        }
    
        if total_nodes > 0 {
            outgoing_connections as f64 / total_nodes as f64
        } else {
            0.0 // 0 if the graph has no nodes
        }
    }

    pub fn calculate_inbound(
        orders: &HashMap<Pubkey, HashMap<Pubkey, u64>>,
    ) -> f64 {
        let total_nodes = orders.len();
        let mut inbound_connections = 0;
    
        for source in orders.values() {
            inbound_connections += source.len();
        }
    
        if total_nodes > 0 {
            inbound_connections as f64 / total_nodes as f64
        } else {
            // self.factor = 0.0 // 0 if the graph has no nodes
            0.0 // 0 if the graph has no nodes
        }
    }


}

#[derive(Debug, Clone)]
pub struct SimulationParamaters {
    pub gossip_push_fanout: usize,
    pub gossip_active_set_size: usize,
    pub gossip_iterations: usize, 
    pub origin_rank: usize,
    pub probability_of_rotation: f64,
    pub prune_stake_threshold: f64,
    pub min_ingress_nodes: usize,
    pub fail_nodes: bool,
    pub fraction_to_fail: f64,
    pub when_to_fail: usize,
    pub test_type: Testing,
    pub num_simulations: usize,
    pub step_size: StepSize,
}

impl Default for SimulationParamaters {
    fn default() -> Self {
        Self {
            gossip_push_fanout: 0,
            gossip_active_set_size: 0,
            gossip_iterations: 0, 
            origin_rank: 0,
            probability_of_rotation: 0.0,
            prune_stake_threshold: 0.0,
            min_ingress_nodes: 0,
            fail_nodes: false,
            fraction_to_fail: 0.0,
            when_to_fail: 0,
            test_type: Testing::NoTest,
            num_simulations: 0,
            step_size: StepSize::Integer(0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GossipStats {
    hops_stats: HopsStatCollection,
    coverage_stats: StatCollection,
    relative_message_redundancy_stats: StatCollection,
    stranded_nodes: StrandedNodeCollection,
    outbound_branching_factors: StatCollection,
    inbound_branching_factors: StatCollection,
    origin: Pubkey,
    pub simulation_parameters: SimulationParamaters,
    failed_nodes: HashSet<Pubkey>,
}

impl Default for GossipStats {
    fn default() -> Self {
        GossipStats { 
            hops_stats: HopsStatCollection::default(), 
            coverage_stats: StatCollection::new("Coverage"),
            relative_message_redundancy_stats: StatCollection::new("RMR"),
            stranded_nodes: StrandedNodeCollection::default(),
            outbound_branching_factors: StatCollection::new("Outbound Branching Factor"),
            inbound_branching_factors: StatCollection::new("Inbound Branching Factor"),
            origin: Pubkey::default(),
            simulation_parameters: SimulationParamaters::default(),
            failed_nodes: HashSet::default(),
        }
    }
}

impl GossipStats {
    pub fn set_origin(
        &mut self,
        origin: Pubkey,
    ) {
        self.origin = origin;
    }

    pub fn origin(
        &self,
    ) -> Pubkey {
        self.origin
    }

    pub fn set_simulation_parameters(
        &mut self,
        config: &Config,
    ) {
        self.simulation_parameters = SimulationParamaters {
            gossip_push_fanout: config.gossip_push_fanout,
            gossip_active_set_size: config.gossip_active_set_size,
            origin_rank: config.origin_rank,
            gossip_iterations: config.gossip_iterations,
            probability_of_rotation: config.probability_of_rotation,
            prune_stake_threshold: config.prune_stake_threshold,
            min_ingress_nodes: config.min_ingress_nodes, 
            fail_nodes: config.fail_nodes,
            fraction_to_fail: config.fraction_to_fail,
            when_to_fail: config.when_to_fail,
            test_type: config.test_type,
            num_simulations: config.num_simulations,
            step_size: config.step_size,
        }  
    }

    pub fn insert_hops_stat(
        &mut self,
        distances: &HashMap<Pubkey, u64>,
    ) {
        self.hops_stats.insert(
            &mut distances
                .values()
                .cloned()
                .collect());
    }

    pub fn print_hops_stats(
        &self,
    ) {
        info!("|------------------------|");
        info!("|------ HOPS STATS ------|");
        info!("|------------------------|");         
        for (iteration, stat) in self.hops_stats
            .per_round_stats
            .iter()
            .enumerate() {
                info!("Iteration: {}", iteration);
                stat.print_stats();
        }
    }

    pub fn get_per_hop_stats_by_index(
        &self,
        iteration: usize,
    ) -> (f64, f64, u64, u64) {
        (
            self.hops_stats.per_round_stats[iteration].mean(), 
            self.hops_stats.per_round_stats[iteration].median(), 
            self.hops_stats.per_round_stats[iteration].max(), 
            self.hops_stats.per_round_stats[iteration].min(), 
        )
    }

    pub fn build_aggregate_hops_stats_histogram(
        &mut self,
        num_buckets: u64,
    ) {
        self.hops_stats.build_histogram(num_buckets);
    }

    fn print_histogram(
        &self,
        hist_type: String,
        histogram: &Histogram,
    ) {
        info!("|------------------------------------------------|");
        info!("|---- {} HISTOGRAM W/ {} BUCKETS ----|", hist_type, histogram.num_buckets());
        info!("|------------------------------------------------|"); 
        // Print the histogram sorted by bucket index
        for (bucket, count) in histogram.entries.iter() {
            let bucket_min = histogram.min_entry() + bucket * histogram.bucket_range();
            let bucket_max = histogram.min_entry() + (bucket + 1) * histogram.bucket_range() - 1;
            if bucket_min == bucket_max {
                info!("Bucket: {}: Count: {}", bucket_max, count);
            } else {
                info!("Bucket: {}-{}: Count: {}", bucket_min, bucket_max, count);
            }

        }
    }

    pub fn print_aggregate_hops_stats_histogram(
        &self,
    ) {
        self.print_histogram(
            "HOPS STATS".to_string(),
            self.hops_stats.get_histogram()
        )
    }

    pub fn calculate_aggregate_hop_stats(
        &mut self,
    ) {
        self.hops_stats.aggregate_hop_stats();
    }

    pub fn print_aggregate_hop_stats(
        &self,
    ) {
        info!("|---------------------------------|");
        info!("|------ AGGREGATE HOP STATS ------|");
        info!("|---------------------------------|");     
        let stats = self.hops_stats.get_aggregate_hop_stats();
        info!("Aggregate Hops {}", stats.mean);
        info!("Aggregate Hops {}", stats.median);
        info!("Aggregate Hops {}", stats.max);
    }

    pub fn get_aggregate_hop_stats(
        &mut self,
    ) -> (f64, f64, u64, u64) {
        let stats = self.hops_stats.get_aggregate_hop_stats();
        (
            stats.mean(),
            stats.median(),
            stats.max(),
            stats.min(),
        )
    }

    pub fn calculate_last_delivery_hop_stats(
        &mut self,
    ) {
        self.hops_stats.calc_last_delivery_hop_stats();
    }

    pub fn print_last_delivery_hop_stats(
        &self,
    ) {
        info!("|-------------------------------------|");
        info!("|------ LAST DELIVERY HOP STATS ------|");
        info!("|-------------------------------------|");     
        let stats = self.hops_stats.get_last_delivery_hop_stats();
        info!("LDH {}", stats.mean);
        info!("LDH {}", stats.median);
        info!("LDH {}", stats.max);
        info!("LDH {}", stats.min);
    }

    pub fn get_last_delivery_hop_stats(
        &mut self,
    ) -> (f64, f64, u64, u64) {
        self.hops_stats.calc_last_delivery_hop_stats();
        let stats = self.hops_stats.get_last_delivery_hop_stats();
        (
            stats.mean(),
            stats.median(),
            stats.max(),
            stats.min(),
        )
    }

    pub fn insert_coverage(
        &mut self,
        value: f64,
    ) {
        self.coverage_stats.collection.push(value);
    }

    pub fn calculate_coverage_stats(
        &mut self,
    ) {
        self.coverage_stats.calculate_stats();
    }

    pub fn get_coverage_stats(
        &self,
    ) -> (f64, f64, f64, f64) {
        (
            self.coverage_stats.mean(),
            self.coverage_stats.median(),
            self.coverage_stats.max(),
            self.coverage_stats.min(),
        )
    }

    pub fn print_coverage_stats(
        &self,
    ) {
        info!("|------------------------|");
        info!("|---- COVERAGE STATS ----|");
        info!("|------------------------|"); 
        self.coverage_stats.print_stats();
    }

    pub fn insert_rmr(
        &mut self,
        rmr: f64,
    ) {
        self.relative_message_redundancy_stats.collection.push(rmr);
    }

    pub fn calculate_rmr_stats(
        &mut self,
    ) {
        self.relative_message_redundancy_stats.calculate_stats();
    }

    pub fn get_rmr_by_index(
        &self,
        index: usize,
    ) -> f64 {
        self.relative_message_redundancy_stats.get_stat_by_index(index)
    }

    pub fn get_rmr_stats(
        &self,
    ) -> (f64, f64, f64, f64) {
        (
            self.relative_message_redundancy_stats.mean(),
            self.relative_message_redundancy_stats.median(),
            self.relative_message_redundancy_stats.max(),
            self.relative_message_redundancy_stats.min(),
        )
    }

    pub fn print_rmr_stats(
        &self,
    ) {
        info!("|-------------------------------------------------|");
        info!("|---- RELATIVE MESSAGE REDUNDANCY (RMR) STATS ----|");
        info!("|-------------------------------------------------|"); 
        self.relative_message_redundancy_stats.print_stats();
    }

    pub fn insert_stranded_nodes(
        &mut self,
        stranded_nodes: &Vec<Pubkey>,
        stakes: &HashMap<Pubkey, u64>,
    ) {
        self.stranded_nodes.insert_nodes(stranded_nodes, stakes);
    }

    pub fn get_stranded_nodes(
        &self,
    ) -> Vec<(Pubkey, (u64, u64))> {
        self.stranded_nodes.get_sorted_stranded()
    }

    pub fn print_stranded(
        &self,
    ) {
        info!("|----------------------------------------------------------|");
        info!("|---- STRANDED NODES (Pubkey, stake, # times stranded) ----|");
        info!("|----------------------------------------------------------|"); 
        info!("Total stranded nodes: {}", self.stranded_nodes.stranded_count());
        for (node, (stake, count)) in self.stranded_nodes.get_sorted_stranded().iter() {
            if stake == &0 {
                info!("{:?},\t{},\t\t{}", node, stake, count);
            } else {
                info!("{:?},\t{},\t{}", node, stake, count);
            }
        }
    }

    // must call: calculate_stranded_stats() before calling this method
    pub fn get_stranded_stats(
        &mut self,
    ) -> (
        u64, // total stranded iterations
        f64, // on average how many iterations was a gossip node stranded across our test
        f64, // avg number of nodes stranded during each gossip iteration
        f64, // avg number of iterations a stranded node was stranded for 
        f64, // median number of iterations a stranded node was stranded for 
        f64, // mean stake of stranded nodes
        f64, // median stake of stranded nodes
        u64, // max stake of stranded nodes
        u64, // min stake of stranded nodes
        f64, // weighted mean stake of stranded nodes
        f64, // weighted median stake of stranded nodes
    ) {
        let stake_stats = self.stranded_nodes.get_stranded_stake_stats();
        let weighted_stake_stats = self.stranded_nodes.get_weighted_stranded_stake_stats();
        (
            self.stranded_nodes.get_total_stranded_iterations(),
            self.stranded_nodes.get_stranded_iterations_per_node(),
            self.stranded_nodes.get_mean_stranded_per_iteration(),
            self.stranded_nodes.get_mean_standed_iterations_per_stranded_node(),
            self.stranded_nodes.get_median_standed_iterations_per_stranded_node(),
            stake_stats.0, 
            stake_stats.1, 
            stake_stats.2, 
            stake_stats.3, 
            weighted_stake_stats.0,
            weighted_stake_stats.1,
        )
    }


    pub fn calculate_stranded_stats(
        &mut self,
    ) {
        self.stranded_nodes.calculate_stats();
    }

    pub fn print_stranded_stats(
        &self,
    ) {
        info!("|-----------------------------|");
        info!("|---- STRANDED NODE STATS ----|");
        info!("|-----------------------------|"); 
        info!("Total stranded node iterations -> SUM(stranded_node_iterations): {}", self.stranded_nodes.get_total_stranded_iterations());
        // total_stranded_iterations / all gossip nodes
        // on average how many iterations was a gossip node stranded across our test
        // may not be great stat since median is likely to be 0 every time
        info!("Mean number of iterations a gossip node was stranded for: {:.6}", self.stranded_nodes.get_stranded_iterations_per_node());
        
        // avg number of nodes stranded during each gossip iteration
        info!("Mean number of nodes stranded during each gossip iteration: {:.6}", self.stranded_nodes.get_mean_stranded_per_iteration());
        
        // avg number of iterations a stranded node was stranded for 
        info!("Mean number of iterations a stranded node was stranded for: {:.6}", self.stranded_nodes.get_mean_standed_iterations_per_stranded_node());
        
        // median number of iterations a stranded node was stranded for  
        info!("Median number of iterations a stranded node was stranded for: {}", self.stranded_nodes.get_median_standed_iterations_per_stranded_node());

        let stake_stats = self.stranded_nodes.get_stranded_stake_stats();
        info!("Mean stake: {:.2}", stake_stats.0);
        info!("Median stake: {}", stake_stats.1);
        info!("Max stake: {}", stake_stats.2);
        info!("Min stake: {}", stake_stats.3);

        let weighted_stake_stakes = self.stranded_nodes.get_weighted_stranded_stake_stats();
        info!("Mean Weighted stake: {:.2}", weighted_stake_stakes.0);
        info!("Median Weighted stake: {}", weighted_stake_stakes.1);
    }

    pub fn build_stranded_node_histogram(
        &mut self,
        num_buckets: u64,
    ) {
        self.stranded_nodes.build_histogram(num_buckets);
    }

    pub fn print_stranded_node_histogram(
        &self,
    ) {
        self.print_histogram(
            "STRANDED NODES".to_string(),
            self.stranded_nodes.get_histogram()
        )
    }

    pub fn print_branching_factor_stats(
        &self,
    ) {
        info!("|-----------------------------------|");
        info!("|---- OUTBOUND BRANCHING FACTOR ----|");
        info!("|-----------------------------------|"); 
        self.outbound_branching_factors.print_stats();
        info!("|----------------------------------|");
        info!("|---- INBOUND BRANCHING FACTOR ----|");
        info!("|----------------------------------|"); 
        self.inbound_branching_factors.print_stats();
    }

    pub fn calculate_outbound_branching_factor(
        &mut self,
        pushes: &HashMap<Pubkey, HashSet<Pubkey>>,
    ) {
        self.outbound_branching_factors
            .collection
            .push(
                BranchingFactor::calculate_outbound(pushes));

    }

    pub fn calculate_inbound_branching_factor(
        &mut self,
        orders: &HashMap<Pubkey, HashMap<Pubkey, u64>>,
    ) {
        self.inbound_branching_factors
            .collection
            .push(
                BranchingFactor::calculate_inbound(orders));

    }

    pub fn get_outbound_branching_factor_by_index(
        &self,
        index: usize,
    ) -> f64 {
        self.outbound_branching_factors.get_stat_by_index(index)
    }

    pub fn calculate_branching_factor_stats(
        &mut self,
    ) {
        self.outbound_branching_factors.calculate_stats();
        self.inbound_branching_factors.calculate_stats();
    }

    // not efficient but this is just for stat recording
    pub fn set_failed_nodes(
        &mut self,
        failed_nodes: &HashSet<Pubkey>,
    ) {
        for key in failed_nodes.iter() {
            self.failed_nodes.insert(*key);
        }
    }

    pub fn print_failed_nodes(
        &self,
    ) {
        info!("|----------------------|");
        info!("|---- FAILED NODES ----|");
        info!("|----------------------|"); 
        info!("Total Failed: {}", self.failed_nodes.len());
        for key in self.failed_nodes.iter() {
            debug!("{:?}", key);
        }
    }

    pub fn run_all_calculations(
        &mut self,
        num_buckets: u64,
    ) {
        self.calculate_coverage_stats();
        self.calculate_rmr_stats();
        self.calculate_aggregate_hop_stats();
        self.build_aggregate_hops_stats_histogram(num_buckets);
        self.calculate_last_delivery_hop_stats();
        self.calculate_stranded_stats();
        self.build_stranded_node_histogram(num_buckets);
        self.calculate_branching_factor_stats();

    }

    pub fn print_all(
        &self,
    ) {
        self.print_coverage_stats();
        self.print_rmr_stats();
        self.print_aggregate_hop_stats();
        self.print_aggregate_hops_stats_histogram();
        self.print_last_delivery_hop_stats();
        self.print_stranded_stats();
        self.print_stranded_node_histogram();
        self.print_stranded();
        self.print_failed_nodes();
        self.print_branching_factor_stats();
        // self.print_hops_stats();
    }
}

pub struct GossipStatsCollection {
    gossip_stats_collection: Vec<GossipStats>,
    num_sims: usize,
    origin: Pubkey,
}

impl Default for GossipStatsCollection {
    fn default() -> Self {
        GossipStatsCollection { 
            gossip_stats_collection: Vec::default(),
            num_sims: 0,
            origin: Pubkey::default(),
        }
    }
}

impl GossipStatsCollection {
    pub fn set_number_of_simulations(
        &mut self,
        num_sims: usize,
    ) {
        self.num_sims = num_sims;
    }

    pub fn set_origin(
        &mut self,
        origin: Pubkey
    ) {
        if origin == Pubkey::default() {
            self.origin = origin;
        }
    }

    pub fn push (
        &mut self,
        gossip_stat: GossipStats,
    ) {
        self.gossip_stats_collection.push(gossip_stat);
    }

    fn get_total_stranded_iterations(
        &self,
    ) {
        let mut total_stranded_iterations: u64 = 0;
        for gossip_stat in self.gossip_stats_collection.iter() {
            total_stranded_iterations += gossip_stat.stranded_nodes.get_total_stranded_iterations();
        }
        info!("Total stranded node iterations across all simulations {}", total_stranded_iterations);
    }

    pub fn print_all(
        &self,
        gossip_iterations: usize,
        test_type: Testing,
    ) {
        info!("|----------------------------------------------------------|");
        info!("|--- GOSSIP STATS COLLECTION ACROSS ALL {} SIMULATION(S) ---|", self.num_sims);
        info!("|--- Gossip Iterations: {} ", gossip_iterations);
        info!("|--- Test Type: {} ", test_type);
        info!("|----------------------------------------------------------|"); 
        for (iteration, stat) in self.gossip_stats_collection.iter().enumerate() {
            info!("|#######################################################################################|");
            info!("Simulation Iteration: {}, Origin: {}", iteration, stat.origin());
            info!("{:#?}", stat.simulation_parameters);
            stat.print_all()
        }

        self.get_total_stranded_iterations();
    }
}


#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use crate::gossip::{Cluster, make_gossip_cluster_for_tests, Node};

    use {
        super::*,
        rand::SeedableRng, rand_chacha::ChaChaRng, std::iter::repeat_with,
        rand::Rng,
        solana_sdk::{pubkey::Pubkey},
        std::{
            collections::{HashMap},
        },
        solana_sdk::native_token::LAMPORTS_PER_SOL,
        rand::rngs::StdRng,
    };

    pub fn calc_coverage(
        stakes: &HashMap<Pubkey, u64>,
        distances: &HashMap<Pubkey, u64>,
    ) -> f64 {
        let num_visited = distances
            .values()
            .filter(|&value| *value != u64::MAX)
            .count();

        num_visited as f64 / stakes.len() as f64
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
    fn test_stranded() {
        let nodes: Vec<_> = repeat_with(Pubkey::new_unique).take(9).collect();
        const MAX_STAKE: u64 = (1 << 20) * LAMPORTS_PER_SOL;
        let mut rng = ChaChaRng::from_seed([189u8; 32]);
        let pubkey = Pubkey::new_unique();
        let stakes = repeat_with(|| rng.gen_range(1, MAX_STAKE));
        let mut stakes: HashMap<_, _> = nodes.iter().copied().zip(stakes).collect();
        stakes.insert(pubkey, rng.gen_range(1, MAX_STAKE));

        for (key, stake) in stakes.iter() {
            println!("{:?}, {}", key, stake);
        }
        let mut gossip_stats = GossipStats::default();
        let mut stranded_nodes: Vec<Pubkey> = Vec::default();

        // stranded
        stranded_nodes.push(Pubkey::from_str("11111113pNDtm61yGF8j2ycAwLEPsuWQXobye5qDR").unwrap());
        stranded_nodes.push(Pubkey::from_str("11111114DhpssPJgSi1YU7hCMfYt1BJ334YgsffXm").unwrap());
        stranded_nodes.push(Pubkey::from_str("11111114d3RrygbPdAtMuFnDmzsN8T5fYKVQ7FVr7").unwrap());
        stranded_nodes.push(Pubkey::from_str("111111152P2r5yt6odmBLPsFCLBrFisJ3aS7LqLAT").unwrap());

        gossip_stats.insert_stranded_nodes(&stranded_nodes, &stakes);
        gossip_stats.calculate_stranded_stats();
        let stranded_stats = gossip_stats.get_stranded_stats();
        assert_eq!(stranded_stats.0, 4);
        assert_eq!(stranded_stats.1, 0.4);
        assert_eq!(stranded_stats.2, 4.0);
        assert_eq!(stranded_stats.3, 1.0);
        assert_eq!(stranded_stats.4, 1.0);
        assert_eq!(stranded_stats.5, 645017127080371.25);
        assert_eq!(stranded_stats.6, 724161057685112.0);
        assert_eq!(stranded_stats.7, 1017190976849038);
        assert_eq!(stranded_stats.8, 114555416102223);
        assert_eq!(stranded_stats.9, 645017127080371.25);
        assert_eq!(stranded_stats.10, 724161057685112.0);

        for _ in 0..4 {
            stranded_nodes.push(Pubkey::from_str("11111113R2cuenjG5nFubqX9Wzuukdin2YfGQVzu5").unwrap());
            stranded_nodes.push(Pubkey::from_str("11111112D1oxKts8YPdTJRG5FzxTNpMtWmq8hkVx3").unwrap());
            stranded_nodes.push(Pubkey::from_str("111111131h1vYVSYuKP6AhS86fbRdMw9XHiZAvAaj").unwrap());
            stranded_nodes.push(Pubkey::from_str("1111111QLbz7JHiBTspS962RLKV8GndWFwiEaqKM").unwrap());
        }

        for _ in 0..7 {
            stranded_nodes.push(Pubkey::from_str("11111113R2cuenjG5nFubqX9Wzuukdin2YfGQVzu5").unwrap());
            stranded_nodes.push(Pubkey::from_str("111111152P2r5yt6odmBLPsFCLBrFisJ3aS7LqLAT").unwrap());
            stranded_nodes.push(Pubkey::from_str("1111111QLbz7JHiBTspS962RLKV8GndWFwiEaqKM").unwrap());
            stranded_nodes.push(Pubkey::from_str("11111114DhpssPJgSi1YU7hCMfYt1BJ334YgsffXm").unwrap());
        }

        gossip_stats.insert_stranded_nodes(&stranded_nodes, &stakes);
        gossip_stats.calculate_stranded_stats();
        let stranded_stats = gossip_stats.get_stranded_stats();
        assert_eq!(stranded_stats.0, 52);
        assert_eq!(stranded_stats.1, 5.2);
        assert_eq!(stranded_stats.2, 26.0);
        assert_eq!(stranded_stats.3, 6.50);
        assert_eq!(stranded_stats.4, 6.50);
        assert_eq!(stranded_stats.5, 617812196595019.00);
        assert_eq!(stranded_stats.6, 623567922929968.5);
        assert_eq!(stranded_stats.7, 1017190976849038);
        assert_eq!(stranded_stats.8, 114555416102223);
        assert_eq!(stranded_stats.9, 615709255382738.9);
        assert_eq!(stranded_stats.10, 585038762479069.0);
    }

    #[test]
    fn test_rmr() {
        const PUSH_FANOUT: usize = 2;
        const ACTIVE_SET_SIZE: usize = 12;
        const PRUNE_STAKE_THRESHOLD: f64 = 0.15;
        const MIN_INGRESS_NODES: usize = 2;
        const CHANCE_TO_ROTATE: f64 = 0.2;
        const GOSSIP_ITERATIONS: usize = 100;

        let nodes: Vec<_> = repeat_with(Pubkey::new_unique).take(5).collect();
        const MAX_STAKE: u64 = (1 << 20) * LAMPORTS_PER_SOL;
        let mut rng = ChaChaRng::from_seed([189u8; 32]);
        let pubkey = Pubkey::new_unique();
        let stakes = repeat_with(|| rng.gen_range(1, MAX_STAKE));
        let mut stakes: HashMap<_, _> = nodes.iter().copied().zip(stakes).collect();
        stakes.insert(pubkey, rng.gen_range(1, MAX_STAKE));

        for (key, stake) in stakes.iter() {
            println!("{:?}, {}", key, stake);
        }
        let mut gossip_stats = GossipStats::default();
        let mut cluster = Cluster::new(PUSH_FANOUT);
        let origin_pubkey = &pubkey; //just a temp origin selection


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

        for _ in 0..GOSSIP_ITERATIONS {
            {
                let node_map: HashMap<Pubkey, &Node> = nodes
                    .iter()
                    .map(|node| (node.pubkey(), node))
                    .collect();
                cluster.new_mst(origin_pubkey, &stakes, &node_map);
            }

            match cluster.relative_message_redundancy() {
                Ok(result) => {
                    gossip_stats.insert_rmr(result);
                },
                Err(_) => error!("Network RMR error. # of nodes is 1."),
            }

            cluster.consume_messages(origin_pubkey, &mut nodes);
            cluster.send_prunes(*origin_pubkey, &mut nodes, PRUNE_STAKE_THRESHOLD, MIN_INGRESS_NODES, &stakes);

            {
                let node_map: HashMap<Pubkey, &Node> = nodes
                    .iter()
                    .map(|node| (node.pubkey(), node))
                    .collect();
                cluster.prune_connections(&node_map, &stakes);
            }

            let seed = [42u8; 32];
            let mut rotate_seed_rng = StdRng::from_seed(seed);
            let mut rotate_seed_rng_2 = StdRng::from_seed(seed);
            cluster.chance_to_rotate(&mut rotate_seed_rng_2, &mut nodes, ACTIVE_SET_SIZE, &stakes, CHANCE_TO_ROTATE, &mut rotate_seed_rng);
        
            
        }

        assert_eq!(gossip_stats.get_rmr_by_index(0), 2.8);
        assert_eq!(gossip_stats.get_rmr_by_index(95), 2.0);

        gossip_stats.calculate_rmr_stats();
        let rmr_stats = gossip_stats.get_rmr_stats();
        assert_eq!(rmr_stats.0, 2.4800000000000044); //mean
        assert_eq!(rmr_stats.1, 2.8); //median
        assert_eq!(rmr_stats.2, 2.8); //max
        assert_eq!(rmr_stats.3, 2.0); //min


    }

    #[test]
    fn test_hops() {
        let nodes: Vec<_> = repeat_with(Pubkey::new_unique).take(9).collect();
        const MAX_STAKE: u64 = (1 << 20) * LAMPORTS_PER_SOL;
        let mut rng = ChaChaRng::from_seed([189u8; 32]);
        let pubkey = Pubkey::new_unique();
        let stakes = repeat_with(|| rng.gen_range(1, MAX_STAKE));
        let mut stakes: HashMap<_, _> = nodes.iter().copied().zip(stakes).collect();
        stakes.insert(pubkey, rng.gen_range(1, MAX_STAKE));

        for (key, stake) in stakes.iter() {
            println!("{:?}, {}", key, stake);
        }
        let mut gossip_stats = GossipStats::default();

        let mut distances: HashMap<Pubkey, u64> = HashMap::default();

        // stranded
        distances.insert(Pubkey::from_str("11111113pNDtm61yGF8j2ycAwLEPsuWQXobye5qDR").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("11111114DhpssPJgSi1YU7hCMfYt1BJ334YgsffXm").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("11111114d3RrygbPdAtMuFnDmzsN8T5fYKVQ7FVr7").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("111111152P2r5yt6odmBLPsFCLBrFisJ3aS7LqLAT").unwrap(), u64::MAX);


        // connected
        distances.insert(Pubkey::from_str("11111113R2cuenjG5nFubqX9Wzuukdin2YfGQVzu5").unwrap(), 0);
        distances.insert(Pubkey::from_str("11111112D1oxKts8YPdTJRG5FzxTNpMtWmq8hkVx3").unwrap(), 1);
        distances.insert(Pubkey::from_str("111111131h1vYVSYuKP6AhS86fbRdMw9XHiZAvAaj").unwrap(), 1);
        distances.insert(Pubkey::from_str("1111111QLbz7JHiBTspS962RLKV8GndWFwiEaqKM").unwrap(), 2);
        distances.insert(Pubkey::from_str("11111112cMQwSC9qirWGjZM6gLGwW69X22mqwLLGP").unwrap(), 2);
        distances.insert(Pubkey::from_str("1111111ogCyDbaRMvkdsHB3qfdyFYaG1WtRUAfdh").unwrap(), 3);


        gossip_stats.insert_hops_stat(&distances);
        let hop_stats = gossip_stats.get_per_hop_stats_by_index(0);
        assert_eq!(hop_stats.0, 1.8); //mean
        assert_eq!(hop_stats.1, 2.0); //median
        assert_eq!(hop_stats.2, 3); //max
        assert_eq!(hop_stats.3, 1); //min

        distances.clear();

        // stranded
        distances.insert(Pubkey::from_str("11111113pNDtm61yGF8j2ycAwLEPsuWQXobye5qDR").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("111111152P2r5yt6odmBLPsFCLBrFisJ3aS7LqLAT").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("11111112cMQwSC9qirWGjZM6gLGwW69X22mqwLLGP").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("1111111ogCyDbaRMvkdsHB3qfdyFYaG1WtRUAfdh").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("11111114d3RrygbPdAtMuFnDmzsN8T5fYKVQ7FVr7").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("11111114DhpssPJgSi1YU7hCMfYt1BJ334YgsffXm").unwrap(), u64::MAX);

        // connected
        distances.insert(Pubkey::from_str("11111113R2cuenjG5nFubqX9Wzuukdin2YfGQVzu5").unwrap(), 0);
        distances.insert(Pubkey::from_str("11111112D1oxKts8YPdTJRG5FzxTNpMtWmq8hkVx3").unwrap(), 1);
        distances.insert(Pubkey::from_str("111111131h1vYVSYuKP6AhS86fbRdMw9XHiZAvAaj").unwrap(), 1);
        distances.insert(Pubkey::from_str("1111111QLbz7JHiBTspS962RLKV8GndWFwiEaqKM").unwrap(), 2);
        gossip_stats.insert_hops_stat(&distances);
        gossip_stats.print_hops_stats();
        let hop_stats = gossip_stats.get_per_hop_stats_by_index(1);
        assert_eq!(hop_stats.0, 1.3333333333333333); //mean
        assert_eq!(hop_stats.1, 1.0); //median
        assert_eq!(hop_stats.2, 2); //max
        assert_eq!(hop_stats.3, 1); //min

        distances.clear();

        // stranded
        distances.insert(Pubkey::from_str("11111113pNDtm61yGF8j2ycAwLEPsuWQXobye5qDR").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("111111152P2r5yt6odmBLPsFCLBrFisJ3aS7LqLAT").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("11111112cMQwSC9qirWGjZM6gLGwW69X22mqwLLGP").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("11111114d3RrygbPdAtMuFnDmzsN8T5fYKVQ7FVr7").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("11111114DhpssPJgSi1YU7hCMfYt1BJ334YgsffXm").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("111111131h1vYVSYuKP6AhS86fbRdMw9XHiZAvAaj").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("1111111QLbz7JHiBTspS962RLKV8GndWFwiEaqKM").unwrap(), u64::MAX);

        // connected
        distances.insert(Pubkey::from_str("11111113R2cuenjG5nFubqX9Wzuukdin2YfGQVzu5").unwrap(), 0);
        distances.insert(Pubkey::from_str("11111112D1oxKts8YPdTJRG5FzxTNpMtWmq8hkVx3").unwrap(), 1);
        distances.insert(Pubkey::from_str("1111111ogCyDbaRMvkdsHB3qfdyFYaG1WtRUAfdh").unwrap(), 6);
        gossip_stats.insert_hops_stat(&distances);
        let hop_stats = gossip_stats.get_per_hop_stats_by_index(2);
        assert_eq!(hop_stats.0, 3.5); //mean
        assert_eq!(hop_stats.1, 3.5); //median
        assert_eq!(hop_stats.2, 6); //max
        assert_eq!(hop_stats.3, 1); //min

        /* Aggregate Stats */
        gossip_stats.calculate_aggregate_hop_stats();
        let agg_hop_stats = gossip_stats.get_aggregate_hop_stats();
        assert_eq!(agg_hop_stats.0, 2.0); //mean
        assert_eq!(agg_hop_stats.1, 1.5); //median
        assert_eq!(agg_hop_stats.2, 6); //max
        assert_eq!(agg_hop_stats.3, 1); //min

        /* LDH Stats */
        let ldh_stats = gossip_stats.get_last_delivery_hop_stats();
        assert_eq!(ldh_stats.0, 3.6666666666666665); //mean
        assert_eq!(ldh_stats.1, 3.0); //median
        assert_eq!(ldh_stats.2, 6); //max
        assert_eq!(ldh_stats.3, 2); //min

    }

    #[test]
    fn test_coverage() {
        let nodes: Vec<_> = repeat_with(Pubkey::new_unique).take(9).collect();
        const MAX_STAKE: u64 = (1 << 20) * LAMPORTS_PER_SOL;
        let mut rng = ChaChaRng::from_seed([189u8; 32]);
        let pubkey = Pubkey::new_unique();
        let stakes = repeat_with(|| rng.gen_range(1, MAX_STAKE));
        let mut stakes: HashMap<_, _> = nodes.iter().copied().zip(stakes).collect();
        stakes.insert(pubkey, rng.gen_range(1, MAX_STAKE));

        for (key, stake) in stakes.iter() {
            println!("{:?}, {}", key, stake);
        }
        let mut gossip_stats = GossipStats::default();

        let mut distances: HashMap<Pubkey, u64> = HashMap::default();

        // stranded
        distances.insert(Pubkey::from_str("11111113pNDtm61yGF8j2ycAwLEPsuWQXobye5qDR").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("11111114DhpssPJgSi1YU7hCMfYt1BJ334YgsffXm").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("11111114d3RrygbPdAtMuFnDmzsN8T5fYKVQ7FVr7").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("111111152P2r5yt6odmBLPsFCLBrFisJ3aS7LqLAT").unwrap(), u64::MAX);


        // connected
        distances.insert(Pubkey::from_str("11111113R2cuenjG5nFubqX9Wzuukdin2YfGQVzu5").unwrap(), 0);
        distances.insert(Pubkey::from_str("11111112D1oxKts8YPdTJRG5FzxTNpMtWmq8hkVx3").unwrap(), 1);
        distances.insert(Pubkey::from_str("111111131h1vYVSYuKP6AhS86fbRdMw9XHiZAvAaj").unwrap(), 1);
        distances.insert(Pubkey::from_str("1111111QLbz7JHiBTspS962RLKV8GndWFwiEaqKM").unwrap(), 2);
        distances.insert(Pubkey::from_str("11111112cMQwSC9qirWGjZM6gLGwW69X22mqwLLGP").unwrap(), 2);
        distances.insert(Pubkey::from_str("1111111ogCyDbaRMvkdsHB3qfdyFYaG1WtRUAfdh").unwrap(), 3);

        let coverage: f64 = calc_coverage(&stakes, &distances);
        assert_eq!(coverage, 6.0/10.0 as f64);

        gossip_stats.insert_coverage(coverage);
        gossip_stats.calculate_coverage_stats();
        let coverage_stats = gossip_stats.get_coverage_stats();
        assert_eq!(coverage_stats.0, 6.0/10.0 as f64);
        assert_eq!(coverage_stats.1, 6.0/10.0 as f64);
        assert_eq!(coverage_stats.2, 6.0/10.0 as f64);
        assert_eq!(coverage_stats.3, 6.0/10.0 as f64);

        distances.clear();

        // stranded
        distances.insert(Pubkey::from_str("11111113pNDtm61yGF8j2ycAwLEPsuWQXobye5qDR").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("111111152P2r5yt6odmBLPsFCLBrFisJ3aS7LqLAT").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("11111112cMQwSC9qirWGjZM6gLGwW69X22mqwLLGP").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("1111111ogCyDbaRMvkdsHB3qfdyFYaG1WtRUAfdh").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("11111114d3RrygbPdAtMuFnDmzsN8T5fYKVQ7FVr7").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("11111114DhpssPJgSi1YU7hCMfYt1BJ334YgsffXm").unwrap(), u64::MAX);

        // connected
        distances.insert(Pubkey::from_str("11111113R2cuenjG5nFubqX9Wzuukdin2YfGQVzu5").unwrap(), 0);
        distances.insert(Pubkey::from_str("11111112D1oxKts8YPdTJRG5FzxTNpMtWmq8hkVx3").unwrap(), 1);
        distances.insert(Pubkey::from_str("111111131h1vYVSYuKP6AhS86fbRdMw9XHiZAvAaj").unwrap(), 1);
        distances.insert(Pubkey::from_str("1111111QLbz7JHiBTspS962RLKV8GndWFwiEaqKM").unwrap(), 2);



        let coverage: f64 = calc_coverage(&stakes, &distances);
        assert_eq!(coverage, 4.0/10.0 as f64);

        gossip_stats.insert_coverage(coverage);
        gossip_stats.calculate_coverage_stats();
        let coverage_stats = gossip_stats.get_coverage_stats();
        assert_eq!(coverage_stats.0, 5.0/10.0 as f64);
        assert_eq!(coverage_stats.1, 5.0/10.0 as f64);
        assert_eq!(coverage_stats.2, 6.0/10.0 as f64);
        assert_eq!(coverage_stats.3, 4.0/10.0 as f64);

        distances.clear();

        // stranded
        distances.insert(Pubkey::from_str("11111113pNDtm61yGF8j2ycAwLEPsuWQXobye5qDR").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("111111152P2r5yt6odmBLPsFCLBrFisJ3aS7LqLAT").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("11111112cMQwSC9qirWGjZM6gLGwW69X22mqwLLGP").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("1111111ogCyDbaRMvkdsHB3qfdyFYaG1WtRUAfdh").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("11111114d3RrygbPdAtMuFnDmzsN8T5fYKVQ7FVr7").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("11111114DhpssPJgSi1YU7hCMfYt1BJ334YgsffXm").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("111111131h1vYVSYuKP6AhS86fbRdMw9XHiZAvAaj").unwrap(), u64::MAX);
        distances.insert(Pubkey::from_str("1111111QLbz7JHiBTspS962RLKV8GndWFwiEaqKM").unwrap(), u64::MAX);

        // connected
        distances.insert(Pubkey::from_str("11111113R2cuenjG5nFubqX9Wzuukdin2YfGQVzu5").unwrap(), 0);
        distances.insert(Pubkey::from_str("11111112D1oxKts8YPdTJRG5FzxTNpMtWmq8hkVx3").unwrap(), 1);

        let coverage: f64 = calc_coverage(&stakes, &distances);
        assert_eq!(coverage, 2.0/10.0 as f64);

        gossip_stats.insert_coverage(coverage);
        gossip_stats.calculate_coverage_stats();
        let coverage_stats = gossip_stats.get_coverage_stats();
        assert_eq!(coverage_stats.0, 0.4000000000000001 as f64);
        assert_eq!(coverage_stats.1, 4.0/10.0 as f64);
        assert_eq!(coverage_stats.2, 6.0/10.0 as f64);
        assert_eq!(coverage_stats.3, 2.0/10.0 as f64);
    }

    #[test]
    fn test_branching_factors() {
        let nodes: Vec<_> = repeat_with(Pubkey::new_unique).take(9).collect();
        const MAX_STAKE: u64 = (1 << 20) * LAMPORTS_PER_SOL;
        let mut rng = ChaChaRng::from_seed([189u8; 32]);
        let pubkey = Pubkey::new_unique();
        let stakes = repeat_with(|| rng.gen_range(1, MAX_STAKE));
        let mut stakes: HashMap<_, _> = nodes.iter().copied().zip(stakes).collect();
        stakes.insert(pubkey, rng.gen_range(1, MAX_STAKE));

        for (key, stake) in stakes.iter() {
            println!("{:?}, {}", key, stake);
        }
        let mut gossip_stats = GossipStats::default();

        let mut pushes: HashMap<Pubkey, HashSet<Pubkey>> = HashMap::default();

        let n0 = Pubkey::from_str("11111113pNDtm61yGF8j2ycAwLEPsuWQXobye5qDR").unwrap();
        let n1 = Pubkey::from_str("111111152P2r5yt6odmBLPsFCLBrFisJ3aS7LqLAT").unwrap();
        let n2 = Pubkey::from_str("11111112cMQwSC9qirWGjZM6gLGwW69X22mqwLLGP").unwrap();
        let n3 = Pubkey::from_str("1111111ogCyDbaRMvkdsHB3qfdyFYaG1WtRUAfdh").unwrap();

        let n4 = Pubkey::from_str("11111114d3RrygbPdAtMuFnDmzsN8T5fYKVQ7FVr7").unwrap();
        let n5 = Pubkey::from_str("11111114DhpssPJgSi1YU7hCMfYt1BJ334YgsffXm").unwrap();
        let n6 = Pubkey::from_str("111111131h1vYVSYuKP6AhS86fbRdMw9XHiZAvAaj").unwrap();
        let n7 = Pubkey::from_str("1111111QLbz7JHiBTspS962RLKV8GndWFwiEaqKM").unwrap();



        pushes.insert(n0, HashSet::default());
        pushes.insert(n1, HashSet::default());
        pushes.insert(n2, HashSet::default());
        pushes.insert(n3, HashSet::default());
        pushes.insert(n4, HashSet::default());
        pushes.insert(n5, HashSet::default());
        pushes.insert(n6, HashSet::default());
        pushes.insert(n7, HashSet::default());

        let h0 = pushes.get_mut(&n0).unwrap();
        h0.insert(n3);
        h0.insert(n7);
        h0.insert(n4);

        let h1 = pushes.get_mut(&n1).unwrap();
        h1.insert(n5);
        h1.insert(n6);

        let h2 = pushes.get_mut(&n2).unwrap();
        h2.insert(n6);

        let h3 = pushes.get_mut(&n3).unwrap();
        h3.insert(n1);

        let h4 = pushes.get_mut(&n4).unwrap();
        h4.insert(n5);

        let h6 = pushes.get_mut(&n6).unwrap();
        h6.insert(n5);

        let h7 = pushes.get_mut(&n7).unwrap();
        h7.insert(n2);

        gossip_stats.calculate_outbound_branching_factor(&pushes);
        let bf = gossip_stats.get_outbound_branching_factor_by_index(0);
        
        assert_eq!(bf, 1.25 as f64);

    }



}