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

        let hops: Vec<u64> = hops
            .iter()
            .filter(|&v| *v != u64::MAX)
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

    pub fn print_stats (
        &self,
    ) {
        info!("Number of iterations: {}", self.coverages.len());
        // let formatted: String = self.coverages
        //     .iter()
        //     .map(|&f| format!("{:.6}", f))
        //     .collect::<Vec<String>>()
        //     .join("\n");

        // let output = format!("Coverages:\n{}", formatted);
        // info!("{}", output);

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
        // let formatted: String = self.rmrs
        //     .iter()
        //     .map(|&f| format!("{:.6}", f))
        //     .collect::<Vec<String>>()
        //     .join("\n");

        // let output = format!("RMRs:\n{}", formatted);
        // info!("{}", output);

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
    total_iterations: u64,
    total_stranded_iterations: u64, // sum(times_stranded)
    mean_stranded_per_iteration: f64, // sum(times_stranded) / iterations
    median_stranded_per_iteration: f64, // median(times_stranded)
    stranded_iterations_per_node: f64,  // total_stranded_iterations / # of nodes in network
    total_nodes: usize, // for stranded_iterations_per_node
    // stranded_node_mean_stake: Stats,
    // stranded_node_median_stake: Stats,
    // stranded_node_max_stake: Stats,
    // stranded_node_min_stake: Stats,
    
}

impl Default for StrandedNodeCollection {
    fn default() -> Self {
        Self {
            stranded_nodes: HashMap::default(),
            total_iterations: 0,
            total_stranded_iterations: 0,
            mean_stranded_per_iteration: 0.0,
            median_stranded_per_iteration: 0.0,
            stranded_iterations_per_node: 0.0,
            total_nodes: 0,
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
        let mut stranded_counts: Vec<u64> = Vec::new();

        for (_, (_, times_stranded)) in self.stranded_nodes.iter() {
            self.total_stranded_iterations += times_stranded;
            stranded_counts.push(*times_stranded);
        }

        self.mean_stranded_per_iteration = self.total_stranded_iterations as f64 / self.total_iterations as f64;

        stranded_counts.sort();

        self.median_stranded_per_iteration = if stranded_counts.is_empty() {
            0.0
        } else if stranded_counts.len() % 2 == 0 {
            let mid = stranded_counts.len() / 2;
            (stranded_counts[mid - 1] + stranded_counts[mid]) as f64 / 2.0
        } else {
            stranded_counts[stranded_counts.len() / 2] as f64
        };

        info!("stranded iter, total nodes: {}, {}", self.total_stranded_iterations, self.total_nodes);
        self.stranded_iterations_per_node = self.total_stranded_iterations as f64 / self.total_nodes as f64;
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
        self.total_iterations += 1;
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

    pub fn get_stranded(
        &self,
    ) -> &HashMap<Pubkey, (u64, u64)> {
        &self.stranded_nodes
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

    pub fn get_median_stranded_per_iteration(
        &self,
    ) -> f64 {
        self.median_stranded_per_iteration
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
        for (node, (stake, count)) in self.stranded_nodes.get_stranded().iter() {
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
        info!("Mean stranded iterations per node: {:.6}", self.stranded_nodes.get_stranded_iterations_per_node());
        info!("Mean stranded nodes per iteration: {:.6}", self.stranded_nodes.get_mean_stranded_per_iteration());
        info!("Median stranded nodes per iteration: {}", self.stranded_nodes.get_median_stranded_per_iteration());
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

