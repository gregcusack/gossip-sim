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

}

impl InfluxDB {
    pub fn new(endpoint: &str) -> Result<Self, url::ParseError> {
        let url = Url::parse(endpoint)?;
        Ok(
            Self { 
                url,
                database: DATABASE_NAME.to_string(),
                datapoints: "".to_string(),
            }
        )
    }



    pub fn report_data_point(
        &self,
        data: f64,
        stat_type: String,
        gossip_iteration: usize,
        simulation_iteration: usize,
    ) {

        let data_point = format!("{} simulation_iter={},gossip_iter={},data={}",
            stat_type, 
            simulation_iteration,
            gossip_iteration,
            data);
        

        debug!("datapoint: {:?}", data_point);

        let url = self.url.clone();
        let database = self.database.clone();

        std::thread::spawn(move || {
            ReportToInflux::send(url, database, data_point);
        });

    }

    pub fn report_hops_stat_point(
        &self,
        data: &HopsStat,
        gossip_iteration: usize,
        simulation_iteration: usize,
    ) {

        let data_point = format!("{} simulation_iter={},gossip_iter={},mean={},median={},max={}",
            "hops_stat".to_string(), 
            simulation_iteration,
            gossip_iteration,
            data.mean(),
            data.median(),
            data.max()
        );

        debug!("datapoint: {:?}", data_point);

        let url = self.url.clone();
        let database = self.database.clone();

        std::thread::spawn(move || {
            ReportToInflux::send(url, database, data_point);
        });
    }

    pub fn report_stranded_node_point(
        &self,
        data: &StrandedNodeStats,
        gossip_iteration: usize,
        simulation_iteration: usize,
    ) {

        let data_point = format!("{} simulation_iter={},gossip_iter={},count={},mean={},median={},max={},min={}",
            "stranded_node_stats".to_string(), 
            simulation_iteration,
            gossip_iteration,
            data.count(),
            data.mean(),
            data.median(),
            data.max(),
            data.min(),
        );

        debug!("datapoint: {:?}", data_point);

        let url = self.url.clone();
        let database = self.database.clone();

        std::thread::spawn(move || {
            ReportToInflux::send(url, database, data_point);
        });
    }
}