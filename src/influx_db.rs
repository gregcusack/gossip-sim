use {
    url::Url,
    reqwest,
    tokio,
    log::{error, debug, info, trace},
    crate::gossip_stats::{
        HopsStat,
        StrandedNodeStats,
    },
};

pub const DATABASE_NAME: &str = "gossip_stats";

pub struct ReportToInflux {}

impl ReportToInflux {
    #[tokio::main]
    pub async fn send(
        url: Url,
        database: String,
        data_point: String,
    ) {
        let client = reqwest::Client::new();
        let influx_url = url.join("write").unwrap();

        info!("influx url: {:?}", influx_url);
        
        // Send the request without awaiting the response
        let response = 
            client
                .post(influx_url)
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
        info!("url on creation: {:?}", url);
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
        info!("datapoint: {:?}", self.datapoints);

        let url = self.url.clone();
        let database = self.database.clone();
        let datapoints = self.datapoints.clone();

        std::thread::spawn(move || {
            ReportToInflux::send(url, database, datapoints);
        });
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