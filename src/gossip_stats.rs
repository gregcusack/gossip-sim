
use {
    crate::Stats,
    log::info,
};

pub struct GossipStats {
    coverages: Vec<f64>,
    mean: Stats,
    median: Stats,
    max: Stats,
    min: Stats,
}

impl GossipStats {
    pub fn new() -> Self {
        GossipStats {
            coverages: Vec::new(),
            mean: Stats::Mean(0.0),
            median: Stats::Median(0.0),
            max: Stats::Max(0.0),
            min: Stats::Min(0.0),
        }
    }

    pub fn insert(
        &mut self,
        value: f64,
    ) {
        self.coverages.push(value);
    }

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
            .unwrap();
        let min = *self.coverages
            .first()
            .unwrap();

        self.mean = Stats::Mean(mean);
        self.median = Stats::Median(median);
        self.max = Stats::Max(max);
        self.min = Stats::Min(min);
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

