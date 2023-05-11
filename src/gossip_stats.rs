
use {
    crate::{CoverageStats, HopsStats},
    log::info,
    std::collections::HashMap,
    solana_sdk::pubkey::Pubkey,
};

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
        let count = hops.len();

        // Calculate the mean filter out nodes that haven't been visited and have a dist of u64::MAX
        let mean = hops
            .iter()
            .filter(|&v| *v != u64::MAX)
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
        info!("{}", self.mean);
        info!("{}", self.median);
        info!("{}", self.max);
        info!("{}", self.min);
    }
    
}

pub struct CoverageStatsCollection {
    coverages: Vec<f64>,
    mean: CoverageStats,
    median: CoverageStats,
    max: CoverageStats,
    min: CoverageStats,
}

impl Default for CoverageStatsCollection {
    fn default() -> Self {
        CoverageStatsCollection {
            coverages: Vec::default(),
            mean: CoverageStats::Mean(0.0),
            median: CoverageStats::Median(0.0),
            max: CoverageStats::Max(0.0),
            min: CoverageStats::Min(0.0),
        }
    }
}

impl CoverageStatsCollection {
    pub fn calculate_stats (
        &mut self,
    ) {
        self.coverages
            .sort_by(|a, b| a
                    .partial_cmp(b)
                    .unwrap());
        let len = self.coverages.len();
        let mean = self.coverages
            .iter()
            .sum::<f64>() / len as f64;
        let median = if len % 2 == 0 {
            (self.coverages[len / 2 - 1] + self.coverages[len / 2]) / 2.0
        } else {
            self.coverages[len / 2]
        };
        let max = *self.coverages
            .last()
            .unwrap_or(&0.0);
        let min = *self.coverages
            .first()
            .unwrap_or(&0.0);

        self.mean = CoverageStats::Mean(mean);
        self.median = CoverageStats::Median(median);
        self.max = CoverageStats::Max(max);
        self.min = CoverageStats::Min(min);
    }

    pub fn print_stats (
        &self,
    ) {
        info!("Number of iterations: {}", self.coverages.len());
        info!("{}", self.mean);
        info!("{}", self.median);
        info!("{}", self.max);
        info!("{}", self.min);
    }
    
}

pub struct GossipStats {
    hops_stats: HopsStatCollection,
    coverage_stats: CoverageStatsCollection,
}

impl Default for GossipStats {
    fn default() -> Self {
        GossipStats { 
            hops_stats: HopsStatCollection::default(), 
            coverage_stats: CoverageStatsCollection::default() 
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
        for (iteration, stat) in self.hops_stats
            .stats
            .iter()
            .enumerate() {
                info!("For Iteration: {}", iteration);
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
        self.coverage_stats.print_stats();
    }



}

