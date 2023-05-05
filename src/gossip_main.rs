use solana_sdk::blake3::Hash;

use {
    clap::{crate_description, crate_name, App, Arg, ArgMatches},
    log::{error, info},
    gossip_sim::{
        gossip::{make_gossip_cluster_from_rpc, make_gossip_cluster_from_map, Node, Cluster},
        API_MAINNET_BETA,
        Error,
    },
    solana_client::rpc_client::RpcClient,
    solana_sdk::pubkey::Pubkey,
    std::{
        fs::{File}, 
        path::Path, 
        collections::HashMap,
        process::exit,
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
            Arg::with_name("account_file")
                .long("account-file")
                .value_name("PATH")
                .takes_value(true)
                .default_value("")
                .help("yaml of solana accounts to either read from or write to"),
        )
        .arg(
            Arg::with_name("accounts_from_yaml")
                .long("accounts-from-yaml")
                .value_name("VOTE_ACCOUNTS_FROM_FILE")
                .takes_value(false)
                .help("set to read in key/stake pairs from yaml. use with --acount-file <path>"),
        )
        .get_matches()
}

fn run_gossip(
    // nodes: &[RwLock<Node>],
    nodes: &mut Vec<Node>,
    stakes: &HashMap<Pubkey, /*stake:*/ u64>,
) -> Result<(), Error> {
    let mut rng = rand::thread_rng();
    for node in nodes {
        node.run_gossip(&mut rng, stakes);
    }
    
    Ok(())
}

fn main() {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "INFO");
    }
    solana_logger::setup();

    let matches = parse_matches();

    // check if we want to write keys and stakes to a file
    let account_file = matches.value_of("account_file").unwrap_or_default();

    // check if we want to read in pubkeys/stakes from a file
    let get_pubkeys_and_stake_from_file: bool = matches.is_present("accounts_from_yaml");
    let nodes = if get_pubkeys_and_stake_from_file {
        // READ ACCOUNTS FROM FILE
        if account_file.is_empty() {
            error!("Failed to pass in account file to read from with --accounts-from-yaml flag. need --acount-file <path>");
            exit(-1);
        }
        let path = Path::new(&account_file);
        let file = File::open(path).unwrap();

        info!("Reading {}", account_file);
        let accounts: HashMap<String, u64> = serde_yaml::from_reader(file).unwrap();
        info!("{} accounts read in", accounts.len());
        let nodes = make_gossip_cluster_from_map(&accounts);
        nodes

    } else {
        let json_rpc_url =
            gossip_sim::get_json_rpc_url(matches.value_of("json_rpc_url").unwrap_or_default());
        info!("json_rpc_url: {}", json_rpc_url);
        let rpc_client = RpcClient::new(json_rpc_url);
        let nodes = make_gossip_cluster_from_rpc(&rpc_client);
        nodes
    }.unwrap();

    let (mut nodes, _): (Vec<_>, Vec<_>) = nodes
        .into_iter()
        .map(|(node, sender)| {
            info!("pubkey, stake: {:?}, {}", node.pubkey(), node.stake());
            let pubkey = node.pubkey();
            (node, (pubkey, sender))
        })
        .unzip();

    

    // TODO: remove unstaked here?!
    //get all of the stakes here. map node pubkey => stake
    //this includes unstaked nodes! so i guess Behzad's todo wants to remove unstaked
    let stakes: HashMap<Pubkey, /*stake:*/ u64> = nodes
        .iter()
        .map(|node| (node.pubkey(), node.stake()))
        .collect();
    //collect vector of nodes

    let _res = run_gossip(&mut nodes, &stakes).unwrap();


    let node_map: HashMap<Pubkey, &Node> = nodes
        .iter()
        .map(|node| (node.pubkey(), node))
        .collect();

    let mut cluster = Cluster::new();
    let origin_pubkey = &nodes[0].pubkey(); //just a temp origin selection

    info!("Origin pubkey: {:?}", origin_pubkey);
    cluster.new_mst(origin_pubkey, &stakes, &node_map);
    info!("---------------------------");
    cluster.print_results();


}