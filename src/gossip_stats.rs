
use {
    crate::{Stats, HopsStats},
    log::info,
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
        distances: &HashMap<Pubkey, u64>
    ) -> HopsStat {
        let mut hops: Vec<u64> = distances.values().cloned().collect();
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
        info!("Hops {}", self.min);
    }
    
}

// if we run multiple MSTs, this will keep track of the hops
// over the course of multiple runs.
pub struct HopsStatCollection {
    stats: Vec<HopsStat>,
}

impl Default for HopsStatCollection {
    fn default() -> Self {
        Self {
            stats: Vec::default(),
        }
    }
}

impl HopsStatCollection {
    pub fn insert(
        &mut self,
        stat: HopsStat,
    ) {
        self.stats.push(stat);
    }

    pub fn get_stat_by_iteration(
        &self,
        index: usize,
    ) -> &HopsStat {
        &self.stats[index]
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
        info!("Coverages: {}", self.coverages
            .iter()
            .map(|n| format!("{:.6}", n))
            .collect::<Vec<String>>()
            .join(", ")
        );
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
        info!("RMRs: {}", self.rmrs
            .iter()
            .map(|n| format!("{:.6}", n))
            .collect::<Vec<String>>()
            .join(", ")
        );
        info!("RMR {}", self.mean);
        info!("RMR {}", self.median);
        info!("RMR {}", self.max);
        info!("RMR {}", self.min);
    }
}

pub struct GossipStats {
    hops_stats: HopsStatCollection,
    coverage_stats: CoverageStatsCollection,
    relative_message_redundancy_stats: RelativeMessageRedundancyCollection,
}

impl Default for GossipStats {
    fn default() -> Self {
        GossipStats { 
            hops_stats: HopsStatCollection::default(), 
            coverage_stats: CoverageStatsCollection::default(),
            relative_message_redundancy_stats: RelativeMessageRedundancyCollection::default(),
        }
    }
}

impl GossipStats {

    pub fn insert_hops_stat(
        &mut self,
        stat: HopsStat,
    ) {
        self.hops_stats.insert(stat);
    }

    pub fn print_hops_stats(
        &self,
    ) {
        info!("|------------------------|");
        info!("|------ HOPS STATS ------|");
        info!("|------------------------|");         
        for (iteration, stat) in self.hops_stats
            .stats
            .iter()
            .enumerate() {
                info!("Iteration: {}", iteration);
                stat.print_stats();
        }
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

    pub fn print_all(
        &mut self,
    ) {
        self.calculate_coverage_stats();
        self.print_coverage_stats();
        self.calculate_rmr_stats();
        self.print_rmr_stats();
        self.print_hops_stats();
    }

}

