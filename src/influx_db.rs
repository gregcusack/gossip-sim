use {
    url::Url,
    reqwest,
    tokio,
    log::{error, debug, trace},
    crate::gossip_stats::{
        HopsStat,
        StrandedNodeStats,
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
        let client = reqwest::Client::new();
        let influx_url = url.join("write").unwrap();

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

    }
}


#[derive(Clone, Debug)]
pub struct InfluxDB {
    url: Url,
    database: String,
    datapoints: String,
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
                datapoints: "".to_string(),
                username: username,
                password: password,
            }
        )
    }

    pub fn send_data_points(
        &self,
    ) {
        debug!("datapoint: {:?}", self.datapoints);

        let url = self.url.clone();
        let database = self.database.clone();
        let datapoints = self.datapoints.clone();
        let username = self.username.clone();
        let password = self.password.clone();

        ReportToInflux::sender(url, database, username, password, datapoints);
    }

    pub fn create_data_point(
        &mut self,
        data: f64,
        stat_type: String,
        gossip_iteration: usize,
        simulation_iteration: usize,
    ) {
        let data_point = format!("{} simulation_iter={},gossip_iter={},data={}\n",
            stat_type, 
            simulation_iteration,
            gossip_iteration,
            data);
        
        self.datapoints.push_str(data_point.as_str());
    }

    pub fn create_hops_stat_point(
        &mut self,
        data: &HopsStat,
        gossip_iteration: usize,
        simulation_iteration: usize,
    ) {

        let data_point = format!("{} simulation_iter={},gossip_iter={},mean={},median={},max={}\n",
            "hops_stat".to_string(), 
            simulation_iteration,
            gossip_iteration,
            data.mean(),
            data.median(),
            data.max()
        );

        self.datapoints.push_str(data_point.as_str());
    }

    pub fn create_stranded_node_stat_point(
        &mut self,
        data: &StrandedNodeStats,
        gossip_iteration: usize,
        simulation_iteration: usize,
    ) {
        let data_point = format!("{} simulation_iter={},gossip_iter={},count={},mean={},median={},max={},min={}\n",
            "stranded_node_stats".to_string(), 
            simulation_iteration,
            gossip_iteration,
            data.count(),
            data.mean(),
            data.median(),
            data.max(),
            data.min(),
        );

        self.datapoints.push_str(data_point.as_str());
    }
}