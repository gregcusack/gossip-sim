use {
    clap::{crate_description, crate_name, App, Arg, ArgMatches},
    log::info,
    gossip_sim::{
        gossip::{make_gossip_cluster_from_rpc, Node, Packet},
        API_MAINNET_BETA},
    solana_client::rpc_client::RpcClient,
    crossbeam_channel::{Sender},
    std::{
        sync::{Arc}, 
        io::Write, 
        fs::File, 
        path::Path, 
        collections::HashMap,
    },
};

fn parse_matches() -> ArgMatches {
    App::new(crate_name!())
        .about(crate_description!())
        .arg(
            Arg::with_name("json_rpc_url")
                .long("url")
                .value_name("URL_OR_MONIKER")
                .takes_value(true)
                .default_value(API_MAINNET_BETA)
                .help("solana's json rpc url"),
        )
        .arg(
            Arg::with_name("number_of_nodes")
                .long("num-nodes")
                .value_name("NUMBER_OF_NODES_TO_SIMULATE")
                .takes_value(true)
                .default_value(&std::u64::MAX.to_string()) //all vals
                .help("number of nodes to simulate. default is all"),
        )
        .arg(
            Arg::with_name("account_file")
                .long("account-file")
                .value_name("PATH")
                .takes_value(true)
                .default_value("")
                .help("yaml of solana accounts to either read from or write to"),
        )
        .arg(
            Arg::with_name("zero_stakes")
                .long("zero-stakes")
                .value_name("FILTER_STAKES")
                .takes_value(false)
                .help("set if you only want zero-staked nodes"),
        )
        .arg(
            Arg::with_name("remove_zero_staked_nodes")
                .long("filter-zero-staked-nodes")
                .short('f')
                .takes_value(false)
                .help("Filter out all zero-staked nodes"),
        )
        .get_matches()
}

fn write_accounts(
    node: &(Node, Sender<Arc<Packet>>),
    accounts: &mut HashMap<String, u64>,
) {
    info!("pubkey, stake: {:?}, {}", node.0.pubkey(), node.0.stake());
    accounts.insert(
        node.0.pubkey().to_string(), 
        node.0.stake()
    );
}

fn main() {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "INFO");
    }
    solana_logger::setup();

    let matches = parse_matches();

    // get account_file to write accounts to
    let account_file = matches.value_of("account_file").unwrap_or_default();
    let filter_zero_staked_nodes = matches.is_present("remove_zero_staked_nodes");

    // default is just to write stakes pulled from the mainnet API
    let json_rpc_url =
        gossip_sim::get_json_rpc_url(matches.value_of("json_rpc_url").unwrap_or_default());
    info!("json_rpc_url: {}", json_rpc_url);
    let rpc_client = RpcClient::new(json_rpc_url);
    let nodes: Vec<(Node, crossbeam_channel::Sender<Arc<Packet>>)> = make_gossip_cluster_from_rpc(&rpc_client, filter_zero_staked_nodes).unwrap();

    // number of accounts to write to file. will write the first N accounts
    let num_nodes: u64 = matches.value_of("number_of_nodes").unwrap_or_default().parse().unwrap();

    // for testing purposes you can set to write only zero staked accounts to yaml
    let zero_stake = matches.is_present("zero_stakes");

    let mut accounts = HashMap::new();
    let mut account_count: u64 = 0;

    if zero_stake {
        for node in nodes.iter().filter(|node| node.0.stake() == 0) {
            write_accounts(&node, &mut accounts);
            account_count = account_count + 1;
            if account_count > num_nodes - 1 {
                break;
            }
        }
    } else {
        for node in nodes {
            write_accounts(&node, &mut accounts);
            account_count = account_count + 1;
            if account_count > num_nodes - 1 {
                break;
            }
        }
    }

    info!("Writing {}", account_file);
    let serialized = serde_yaml::to_string(&accounts).unwrap();
    let path = Path::new(&account_file);
    let mut file = File::create(path).unwrap();
    file.write_all(&serialized.into_bytes()).unwrap();

    info!("Wrote {} keys to file: {}", account_count, account_file);


}