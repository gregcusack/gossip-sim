use {
    crate::{Stats, HopsStats},
    log::{info, error},
    std::collections::HashMap,
    solana_sdk::pubkey::Pubkey,
};

// stores stats about a single run of mst. 
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
pub struct HopsStatCollection {
    per_round_stats: Vec<HopsStat>,
    raw_hop_collection: Vec<u64>, // all hop counts seen
    aggregate_stats: HopsStat,
    last_delivery_hop_stats: HopsStat,
}

impl Default for HopsStatCollection {
    fn default() -> Self {
        Self {
            per_round_stats: Vec::default(),
            raw_hop_collection: Vec::default(),
            aggregate_stats: HopsStat::default(),
            last_delivery_hop_stats: HopsStat::default(),
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
}

pub struct CoverageStatsCollection {
    coverages: Vec<f64>,
    mean: Stats,
    median: Stats,
    max: Stats,
    min: Stats,
}

impl Default for CoverageStatsCollection {
    fn default() -> Self {
        Self {
            coverages: Vec::default(),
            mean: Stats::Mean(0.0),
            median: Stats::Median(0.0),
            max: Stats::Max(0.0),
            min: Stats::Min(0.0),
        }
    }
}

impl CoverageStatsCollection {
    pub fn calculate_stats (
        &mut self,
    ) {
        // clone to maintain iteration order for print_stats
        let mut sorted_coverages = self.coverages.clone();
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

    pub fn print_stats (
        &self,
    ) {
        info!("Number of iterations: {}", self.coverages.len());
        info!("Coverage {}", self.mean);
        info!("Coverage {}", self.median);
        info!("Coverage {}", self.max);
        info!("Coverage {}", self.min);
    }    
}

// RMR = m / (n - 1) - 1
// m: total number of payload messages exchanged during gossip (push/prune)
// n: total number of nodes that receive the message
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

pub struct RelativeMessageRedundancyCollection {
    rmrs: Vec<f64>,
    mean: Stats,
    median: Stats,
    max: Stats,
    min: Stats,
}

impl Default for RelativeMessageRedundancyCollection {
    fn default() -> Self {
        Self {
            rmrs: Vec::default(),
            mean: Stats::Mean(0.0),
            median: Stats::Median(0.0),
            max: Stats::Max(0.0),
            min: Stats::Min(0.0),
        }
    }
}

impl RelativeMessageRedundancyCollection {
    pub fn calculate_stats (
        &mut self,
    ) {
        // clone to maintain iteration order for print_stats
        let mut sorted_rms = self.rmrs.clone();
        sorted_rms
            .sort_by(|a, b| a
                    .partial_cmp(b)
                    .unwrap());
        let len = sorted_rms.len();
        let mean = sorted_rms
            .iter()
            .sum::<f64>() / len as f64;
        let median = if len % 2 == 0 {
            (sorted_rms[len / 2 - 1] + sorted_rms[len / 2]) / 2.0
        } else {
            sorted_rms[len / 2]
        };
        let max = *sorted_rms
            .last()
            .unwrap_or(&0.0);
        let min = *sorted_rms
            .first()
            .unwrap_or(&0.0);

        self.mean = Stats::Mean(mean);
        self.median = Stats::Median(median);
        self.max = Stats::Max(max);
        self.min = Stats::Min(min);
    }

    pub fn print_stats (
        &self,
    ) {
        info!("Number of iterations: {}", self.rmrs.len());
        info!("RMR {}", self.mean);
        info!("RMR {}", self.median);
        info!("RMR {}", self.max);
        info!("RMR {}", self.min);
    }
}

pub struct StrandedNodeCollection {
    stranded_nodes: HashMap<Pubkey, (/* stake */u64, /* times stranded */ u64)>, 
    /*
    mean stranded nodes per iteration
    median stranded nodes per iteration
    histogram -> # of nodes stranded for n iterations
    total number of stranded node iterations -> number of nodes: 15,000 total iterations stranded.
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
        }
    }
}

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
        let mut stranded_iteration_counts: Vec<u64> = Vec::new();
        let mut stranded_stakes: Vec<u64> = Vec::new();

        for (_, (stake, times_stranded)) in self.stranded_nodes.iter() {
            self.total_stranded_iterations += times_stranded;
            stranded_iteration_counts.push(*times_stranded);

            self.total_stranded_stake += stake;
            stranded_stakes.push(*stake);
        }

        self.mean_stranded_per_iteration = self.total_stranded_iterations as f64 / self.total_gossip_iterations as f64;
        self.stranded_node_mean_stake = self.total_stranded_stake as f64 / self.stranded_count() as f64;
        self.mean_standed_iterations_per_stranded_node = self.total_stranded_iterations as f64 / self.stranded_count() as f64;

        info!("stranded count, total gossip iters: {}, {}", self.stranded_count(), self.total_gossip_iterations);

        stranded_iteration_counts.sort();
        stranded_stakes.sort();

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

        self.stranded_node_max_stake = *stranded_stakes
            .last()
            .unwrap_or(&0);
        self.stranded_node_min_stake = *stranded_stakes
            .first()
            .unwrap_or(&0);

    }

    pub fn insert_nodes(
        &mut self,
        stranded_nodes: Vec<Pubkey>,
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
        &mut self,
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


}

pub struct GossipStats {
    hops_stats: HopsStatCollection,
    coverage_stats: CoverageStatsCollection,
    relative_message_redundancy_stats: RelativeMessageRedundancyCollection,
    stranded_nodes: StrandedNodeCollection,
}

impl Default for GossipStats {
    fn default() -> Self {
        GossipStats { 
            hops_stats: HopsStatCollection::default(), 
            coverage_stats: CoverageStatsCollection::default(),
            relative_message_redundancy_stats: RelativeMessageRedundancyCollection::default(),
            stranded_nodes: StrandedNodeCollection::default(),
        }
    }
}

impl GossipStats {
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

    pub fn print_aggregate_hop_stats(
        &mut self,
    ) {
        info!("|---------------------------------|");
        info!("|------ AGGREGATE HOP STATS ------|");
        info!("|---------------------------------|");     
        self.hops_stats.aggregate_hop_stats();
        let stats = self.hops_stats.get_aggregate_hop_stats();
        info!("Aggregate Hops {}", stats.mean);
        info!("Aggregate Hops {}", stats.median);
        info!("Aggregate Hops {}", stats.max);
    }

    pub fn get_aggregate_hop_stats(
        &mut self,
    ) -> (f64, f64, u64, u64) {
        self.hops_stats.aggregate_hop_stats();
        let stats = self.hops_stats.get_aggregate_hop_stats();
        (
            stats.mean(),
            stats.median(),
            stats.max(),
            stats.min(),
        )
    }

    pub fn print_last_delivery_hop_stats(
        &mut self,
    ) {
        info!("|-------------------------------------|");
        info!("|------ LAST DELIVERY HOP STATS ------|");
        info!("|-------------------------------------|");     
        self.hops_stats.calc_last_delivery_hop_stats();
        let stats = self.hops_stats.get_last_delivery_hop_stats();
        info!("LDH Mean: {}", stats.mean);
        info!("LDH Median: {}", stats.median);
        info!("LDH Max: {}", stats.max);
        info!("LDH Min: {}", stats.min);
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
        self.coverage_stats.coverages.push(value);
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
        self.relative_message_redundancy_stats.rmrs.push(rmr);
    }

    pub fn calculate_rmr_stats(
        &mut self,
    ) {
        self.relative_message_redundancy_stats.calculate_stats();
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
        stranded_nodes: Vec<Pubkey>,
        stakes: &HashMap<Pubkey, u64>,
    ) {
        self.stranded_nodes.insert_nodes(stranded_nodes, stakes);
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

    pub fn print_stranded_stats(
        &mut self,
    ) {
        info!("|-----------------------------|");
        info!("|---- STRANDED NODE STATS ----|");
        info!("|-----------------------------|"); 
        self.stranded_nodes.calculate_stats();
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
    }

    pub fn print_all(
        &mut self,

    ) {
        self.calculate_coverage_stats();
        self.print_coverage_stats();
        self.calculate_rmr_stats();
        self.print_rmr_stats();
        self.print_aggregate_hop_stats();
        self.print_last_delivery_hop_stats();
        self.print_stranded_stats();
        self.print_stranded();
        // self.print_hops_stats();
    }



}


#[cfg(test)]
mod tests {
    use std::str::FromStr;

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

    #[test]
    fn test_hops() {
        let nodes: Vec<_> = repeat_with(Pubkey::new_unique).take(9).collect();
        const MAX_STAKE: u64 = (1 << 20) * LAMPORTS_PER_SOL;
        let mut rng = ChaChaRng::from_seed([189u8; 32]);
        let pubkey = Pubkey::new_unique();
        let stakes = repeat_with(|| rng.gen_range(1, MAX_STAKE));
        let mut stakes: HashMap<_, _> = nodes.iter().copied().zip(stakes).collect();
        stakes.insert(pubkey, rng.gen_range(1, MAX_STAKE));

        println!("stakes len: {}", stakes.len());

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
        gossip_stats.print_hops_stats();
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
        gossip_stats.print_hops_stats();
        let hop_stats = gossip_stats.get_per_hop_stats_by_index(2);
        assert_eq!(hop_stats.0, 3.5); //mean
        assert_eq!(hop_stats.1, 3.5); //median
        assert_eq!(hop_stats.2, 6); //max
        assert_eq!(hop_stats.3, 1); //min

        /* Aggregate Stats */
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

        println!("stakes len: {}", stakes.len());

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

}