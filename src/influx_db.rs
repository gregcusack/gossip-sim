use {
    url::Url,
    reqwest,
    tokio,
    log::{error, info, debug, trace},
    crate::gossip_stats::{
        HopsStat,
        StrandedNodeStats,
    },
    std::{
        time::{SystemTime, UNIX_EPOCH},
        sync::{Arc, Mutex},
        collections::VecDeque,
        thread,
    },

};

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
        info!("in send!");
        let client = reqwest::Client::new();
        let influx_url = url.join("write").unwrap();

        info!("about to send: data_point: {}", data_point);
        info!("url: {:?}", url);

        let response = 
            client
                .post(influx_url)
                .basic_auth(username, Some(password))
                .query(&[("db", database.as_str())])
                .body(data_point)
                .send()
                .await;
        
        info!("suhhhh");

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
    }

    #[tokio::main]
    pub async fn sender(
        url: Url,
        database: String,
        username: String,
        password: String, 
        data_point: String,
    ) {
        info!("in sender(): ");
        async_std::task::spawn(async move {
            ReportToInflux::send(url, database, username, password, data_point);
        });
        // async_std::task::spawn(async move {
        // let _ = ReportToInflux::send(url, database, username, password, data_point);
        // });

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

        loop {
            let datapoint = datapoint_queue.lock().unwrap().pop_front();
            
            if let Some(dp) = datapoint {
                info!("front val: {}", dp.data());
                if dp.last_datapoint() {
                    info!("Last data point, returning");
                    break;
                }

                info!("not last datapoint");
                influx_db.send_data_points(dp);

            } else {
                info!("deque empty");
            }
            thread::sleep(std::time::Duration::from_millis(50));
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
        // let datapoints = self.datapoints.clone();
        let username = self.username.clone();
        let password = self.password.clone();

        info!("sending to sender: ");
        let _ = ReportToInflux::sender(url, database, username, password, datapoint.data());
    }


}

#[derive(Clone, Debug)]
pub struct InfluxDataPoint {
    datapoint: String,
}

impl Default for InfluxDataPoint {
    fn default() -> Self {
        InfluxDataPoint {
            datapoint: "".to_string(),
        }
    }
}

impl InfluxDataPoint {
    pub fn data(
        &self,
    ) -> String {
        self.datapoint.clone()
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
    }

    pub fn append_timestamp(
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
    }

    pub fn create_hops_stat_point(
        &mut self,
        data: &HopsStat,
    ) {

        let data_point = format!("{} mean={},median={},max={}\n",
            "hops_stat".to_string(), 
            data.mean(),
            data.median(),
            data.max()
        );

        self.datapoint.push_str(data_point.as_str());
    }

    pub fn create_stranded_node_stat_point(
        &mut self,
        data: &StrandedNodeStats,
    ) {
        let data_point = format!("{} count={},mean={},median={},max={},min={}\n",
            "stranded_node_stats".to_string(),
            data.count(),
            data.mean(),
            data.median(),
            data.max(),
            data.min(),
        );

        self.datapoint.push_str(data_point.as_str());
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
    }

    pub fn create_config_point(
        &mut self,
        push_fanout: usize,
        active_set_size: usize,
        origin_rank: usize,
        prune_stake_threshold: f64,
        min_ingress_nodes: usize,
    ) {
        let data_point = format!("config push_fanout={},active_set_size={},origin_rank={},prune_stake_threshold={},min_ingress_nodes={}\n",
            push_fanout, 
            active_set_size,
            origin_rank,
            prune_stake_threshold,
            min_ingress_nodes
        );

        self.datapoint.push_str(data_point.as_str());
    }
}