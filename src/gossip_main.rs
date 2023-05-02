use {
    clap::{crate_description, crate_name, App, Arg, ArgMatches, value_t_or_exit},
    log::{error, warn, info},
    gossip_sim::{
        gossip::{make_gossip_cluster, Node, Packet},
        API_MAINNET_BETA},
    solana_client::rpc_client::RpcClient,
    std::{
        sync::{Arc}, 
        io::Write, 
        fs::{self, File}, 
        path::Path, 
        str::FromStr,
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
                .help("yaml of solana accounts to either read from or write to"),
        )
        .arg(
            Arg::with_name("write_stake_accounts")
                .long("write-accounts")
                .value_name("KEYS")
                .takes_value(false)
                .help("set if you just want to write Solana accounts to a file. use with --url"),
        )
        .arg(
            Arg::with_name("zero_stakes")
                .long("zero-stakes")
                .value_name("FILTER_STAKES")
                .takes_value(false)
                .help("set if you only want zero-staked nodes"),
        )
        .get_matches()
}

fn main() {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "INFO");
    }
    solana_logger::setup();

    let matches = parse_matches();

    let mut account_file = "".to_string();
    let write_keys = matches.is_present("write_stake_accounts");
    if write_keys {
        account_file = value_t_or_exit!(matches, "account_file", String);
    }

    let json_rpc_url =
        gossip_sim::get_json_rpc_url(matches.value_of("json_rpc_url").unwrap_or_default());
    info!("json_rpc_url: {}", json_rpc_url);
    let rpc_client = RpcClient::new(json_rpc_url);
    let nodes: Vec<(Node, crossbeam_channel::Sender<Arc<Packet>>)> = make_gossip_cluster(&rpc_client).unwrap();

    let num_nodes: u64 = matches.value_of("number_of_nodes").unwrap_or_default().parse().unwrap();

    let zero_stake = matches.is_present("zero_stakes");

    let mut accounts = HashMap::new();
    let mut account_count: u64 = 0;

    if zero_stake {
        for node in nodes.iter().filter(|node| node.0.stake() == 0) {
            info!("pubkey, stake: {:?}, {}", node.0.pubkey(), node.0.stake());
            if write_keys {
                accounts.insert(
                    node.0.pubkey().to_string(), 
                    node.0.stake()
                );
                account_count = account_count + 1;
                if account_count > num_nodes - 1 {
                    break;
                }
            }
        }
    } else {
        for node in nodes {
            info!("pubkey, stake: {:?}, {}", node.0.pubkey(), node.0.stake());
            if write_keys {
                accounts.insert(
                    node.0.pubkey().to_string(), 
                    node.0.stake()
                );
                account_count = account_count + 1;
                if account_count > num_nodes - 1 {
                    break;
                }
            }
        }
    }

    
    

    if write_keys {
        info!("Writing {}", account_file);
        let serialized = serde_yaml::to_string(&accounts).unwrap();
        let path = Path::new(&account_file);
        let mut file = File::create(path).unwrap();
        file.write_all(&serialized.into_bytes()).unwrap();

        info!("Wrote {} keys to file: {}", account_count, account_file);
    }


}