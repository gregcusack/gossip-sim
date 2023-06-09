use {
    url::Url,
    reqwest,
    tokio,
    log::{error, info, debug, trace},
    crate::gossip_stats::{
        HopsStat,
        StrandedNodeStats,
        Histogram,
    },
    std::{
        time::{SystemTime, UNIX_EPOCH},
        sync::{Arc, Mutex},
        collections::VecDeque,
        thread,
    },

};

static mut TRACKER: Option<Arc<Mutex<Tracker>>> = None;

pub fn get_timestamp_now() -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    format!("{}\n", ts)
}

pub struct ReportToInflux {}

impl ReportToInflux {
    #[tokio::main]
    pub async fn send(
        url: Url,
        database: String,
        username: String,
        password: String, 
        data_point: String,
    ) {
        let client = reqwest::Client::new();
        let influx_url = url.join("write").unwrap();


        debug!("about to send: data_point: {}", data_point);
        debug!("url: {:?}", url);

        let response = 
            client
                .post(influx_url)
                .basic_auth(username, Some(password))
                .query(&[("db", database.as_str())])
                .body(data_point)
                .send()
                .await;
        

        match response {
            Ok(response) => {
                if response.status().is_success() {
                    trace!("Data successfully reported to InfluxDB");
                } else {
                    error!("Failed to report data to InfluxDB. Status: {}", response.status());
                }
            }
            Err(err) => {
                error!("Error reporting to InfluxDB: {}", err);
            }
        }
        unsafe {
            if let Some(ref t) = TRACKER {
                t.lock().unwrap().add_sent();
            } 
        }

    }

    #[tokio::main]
    pub async fn sender(
        url: Url,
        database: String,
        username: String,
        password: String, 
        data_point: String,
    ) {
        async_std::task::spawn(async move {
            ReportToInflux::send(url, database, username, password, data_point);
        });
        // async_std::task::spawn(async move {
        // let _ = ReportToInflux::send(url, database, username, password, data_point);
        // });

    }

}

pub struct Tracker {
    dequeued: usize,
    sent: usize,
}

impl Default for Tracker {
    fn default() -> Self {
        Tracker {
            dequeued: 0,
            sent: 0,
        }
    }
}

impl Tracker {
    pub fn add_dequeued(
        &mut self,
    ) {
        self.dequeued += 1;
    }

    pub fn add_sent(
        &mut self,
    ) {
        self.sent += 1;
    }

    pub fn get_dequeued(
        &self,
    ) -> usize {
        self.dequeued
    }

    pub fn get_sent(
        &self,
    ) -> usize {
        self.sent
    }

    pub fn equal(
        &self,
    ) -> bool {
        self.sent == self.dequeued
    }
}

pub struct InfluxThread { }

impl InfluxThread {
    pub fn start(
        endpoint: &str,
        database: String,
        username: String,
        password: String,
        datapoint_queue: Arc<Mutex<VecDeque<InfluxDataPoint>>>,
    ) {
        let influx_db = InfluxDB::new(
            endpoint,
            database,
            username,
            password
        ).unwrap();

        unsafe {
            TRACKER = Some(Arc::new(Mutex::new(Tracker::default())));
        }

        let mut wait_time = std::time::Duration::from_millis(100);

        let mut rx_last_datapoint = false;
        let mut draining_queue_log_message_flag = false;

        loop {
            let datapoint = datapoint_queue.lock().unwrap().pop_front();
            if let Some(dp) = datapoint {
                if dp.last_datapoint() {
                    rx_last_datapoint = true;
                } else if dp.is_start(){
                    wait_time = std::time::Duration::from_millis(1);
                } else {
                    influx_db.send_data_points(dp);
    
                    unsafe {
                        if let Some(ref t) = TRACKER {
                            t.lock().unwrap().add_dequeued();
                        } 
                    }
                }
            }
            if rx_last_datapoint {
                if !draining_queue_log_message_flag {
                    draining_queue_log_message_flag = true;
                    info!("Last simulation datapoint recorded. Draining Queue...")
                }
                unsafe {
                    if let Some(ref t) = TRACKER {
                        if t.lock().unwrap().equal() {
                            info!("Queue Drained. Exiting...");
                            break;
                        }
                    } 
                }
            }
            thread::sleep(wait_time);
        }
    }
}

#[derive(Clone, Debug)]
pub struct InfluxDB {
    url: Url,
    database: String,
    username: String,
    password: String,
}

impl InfluxDB {
    pub fn new(
        endpoint: &str,
        username: String,
        password: String,
        database: String,

    ) -> Result<Self, url::ParseError> {
        let url = Url::parse(endpoint)?;
        Ok(
            Self { 
                url,
                database: database,
                username: username,
                password: password,
            }
        )
    }

    fn send_data_points(
        &self,
        datapoint: InfluxDataPoint,
    ) {
        debug!("datapoint: {:?}", datapoint);

        let url = self.url.clone();
        let database = self.database.clone();
        let username = self.username.clone();
        let password = self.password.clone();

        let _ = ReportToInflux::sender(url, database, username, password, datapoint.data());
    }


}

#[derive(Clone, Debug)]
pub struct InfluxDataPoint {
    datapoint: String,
    timestamp: String,
}

impl Default for InfluxDataPoint {
    fn default() -> Self {
        InfluxDataPoint {
            datapoint: "".to_string(),
            timestamp: get_timestamp_now(),
        }
    }
}

impl InfluxDataPoint {
    pub fn data(
        &self,
    ) -> String {
        self.datapoint.clone()
    }

    pub fn set_start(
        &mut self,
    ) {
        self.datapoint.push_str("start");
    }

    pub fn is_start(
        &self,
    ) -> bool {
        if self.datapoint == "start" {
            return true;
        }
        false
    }

    pub fn set_last_datapoint(
        &mut self,
    ) {
        self.datapoint.push_str("end");
    }

    pub fn last_datapoint(
        &self,
    ) -> bool {
        if self.datapoint == "end" {
            return true;
        }
        false
    }

    pub fn get_timestamp_now(
        &self,
    ) -> String {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        format!("{}\n", ts)
        // format!("{}", ts)
    }

    pub fn append_timestamp(
        &mut self,
    ) {
        self.datapoint.push_str(self.timestamp.as_str());
    }

    pub fn set_and_append_timestamp(
        &mut self,
    ) {
        self.datapoint.push_str(self.get_timestamp_now().as_str());
    }

    pub fn create_data_point(
        &mut self,
        data: f64,
        stat_type: String,
    ) {
        let data_point = format!("{} data={} ",
            stat_type, 
            data
        );        
        self.datapoint.push_str(data_point.as_str());
        self.append_timestamp();
    }

    pub fn create_hops_stat_point(
        &mut self,
        data: &HopsStat,
    ) {

        let data_point = format!("{} mean={},median={},max={} ",
            "hops_stat".to_string(), 
            data.mean(),
            data.median(),
            data.max()
        );

        self.datapoint.push_str(data_point.as_str());
        self.append_timestamp();
    }

    pub fn create_stranded_node_stat_point(
        &mut self,
        data: &StrandedNodeStats,
    ) {
        let data_point = format!("{} count={},mean={},median={},max={},min={} ",
            "stranded_node_stats".to_string(),
            data.count(),
            data.mean(),
            data.median(),
            data.max(),
            data.min(),
        );

        self.datapoint.push_str(data_point.as_str());
        self.append_timestamp();
    }

    pub fn create_iteration_point(
        &mut self,
        gossip_iter: usize,
        simulation_iter: usize,
    ) {
        let data_point = format!("iteration simulation_iter={},gossip_iter={} ", 
            simulation_iter, 
            gossip_iter
        );
        self.datapoint.push_str(data_point.as_str());
        self.append_timestamp();
    }

    pub fn create_config_point(
        &mut self,
        push_fanout: usize,
        active_set_size: usize,
        origin_rank: usize,
        prune_stake_threshold: f64,
        min_ingress_nodes: usize,
    ) {
        let data_point = format!("config push_fanout={},active_set_size={},origin_rank={},prune_stake_threshold={},min_ingress_nodes={} ",
            push_fanout, 
            active_set_size,
            origin_rank,
            prune_stake_threshold,
            min_ingress_nodes
        );

        self.datapoint.push_str(data_point.as_str());
        self.append_timestamp();
    }

    pub fn create_stranded_iteration_point(
        &mut self,
        total_stranded_iterations_count: u64,
        mean_number_of_iterations_node_stranded_for: f64,        
        mean_number_of_nodes_stranded_during_each_iteration: f64,
        mean_number_of_iterations_a_stranded_node_was_stranded_for: f64,
        median_number_of_iterations_a_stranded_node_was_stranded_for: f64,
        mean_weighted_stake: f64,
        median_weighted_stake: f64,
    ) {
        let data_point = format!("stranded_node_iterations total_stranded={},\
            mean_iter_stranded_per_node={},\
            mean_stranded_per_iter={},\
            mean_iter_stranded={},\
            median_iter_stranded={},\
            mean_weighted_stake={},\
            median_weighted_stake={} ", 
            total_stranded_iterations_count,
            mean_number_of_iterations_node_stranded_for,
            mean_number_of_nodes_stranded_during_each_iteration,
            mean_number_of_iterations_a_stranded_node_was_stranded_for,
            median_number_of_iterations_a_stranded_node_was_stranded_for,
            mean_weighted_stake,
            median_weighted_stake,
            );
        self.datapoint.push_str(data_point.as_str());
        self.append_timestamp();
    }

    pub fn create_histogram_point(
        &mut self,
        data_type: String,
        histogram: &Histogram
    ) {
        for (bucket, count) in histogram.entries().iter() {
            let bucket_min = histogram.min_entry() + bucket * histogram.bucket_range();
            let bucket_max = histogram.min_entry() + (bucket + 1) * histogram.bucket_range() - 1;
            if bucket_min == bucket_max {
                debug!("Bucket: {}: Count: {}", bucket_max, count);
            } else {
                debug!("Bucket: {}-{}: Count: {}", bucket_min, bucket_max, count);
            }
            let data_point  = format!("{} bucket={},count={} ", data_type, bucket_max, count);

            self.datapoint.push_str(data_point.as_str());
            self.set_and_append_timestamp();
        }

        debug!("histogram point: {}", self.datapoint);
    }


}