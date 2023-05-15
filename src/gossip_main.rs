use {
    clap::{crate_description, crate_name, App, Arg, ArgMatches, value_t_or_exit},
    log::{error, info, warn, Level},
    gossip_sim::{
        API_MAINNET_BETA,
        Error,
        gossip::{
            make_gossip_cluster_from_rpc, 
            make_gossip_cluster_from_map,
            Node,
            Cluster,
            Config,
        },
        gossip_stats::{
            GossipStats,
            HopsStat,
        },
    },
    solana_client::rpc_client::RpcClient,
    solana_sdk::pubkey::Pubkey,
    std::{
        fs::{File}, 
        path::Path, 
        collections::{HashMap, BinaryHeap},
        process::exit,
        hash::{
            BuildHasher,
            Hash,
        },
        cmp::Reverse,
    },

    rand::SeedableRng, rand_chacha::ChaChaRng,
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
        .arg(
            Arg::with_name("gossip_push_fanout")
                .long("push-fanout")
                .takes_value(true)
                .default_value("6")
                .help("gossip push fanout"),
        )
        .arg(
            Arg::with_name("gossip_push_active_set_entry_size")
                .long("active-set-size")
                .takes_value(true)
                .default_value("12")
                .help("gossip push active set entry size"),
        )
        .arg(
            Arg::with_name("gossip_iterations")
                .long("iterations")
                .takes_value(true)
                .default_value("1")
                .help("gossip iterations"),
        )
        .arg(
            Arg::with_name("origin_rank")
                .long("origin-rank")
                .takes_value(true)
                .default_value("1")
                .validator(|s| match s.parse::<usize>() {
                    Ok(n) if n >= 1 => Ok(()),
                    _ => Err(String::from("origin_rank must be at least 1")),
                })
                .help("Select an origin with origin rank for gossip.
                    e.g.    10 -> 10th largest stake
                            1000 -> 1000th largest stake
                    Default is largest stake as origin"),
        )
        .get_matches()
}


pub fn run_gossip(
    // nodes: &[RwLock<Node>],
    nodes: &mut Vec<Node>,
    stakes: &HashMap<Pubkey, /*stake:*/ u64>,
    active_set_size: usize,
) -> Result<(), Error> {
    let mut rng = rand::thread_rng();
    // let mut rng = ChaChaRng::from_seed([189u8; 32]);
    for node in nodes {
        node.run_gossip(&mut rng, stakes, active_set_size);
    }
    
    Ok(())
}

fn find_nth_largest_node(n: usize, nodes: &[Node]) -> Option<&Node> {
    let mut heap = BinaryHeap::new();
    for node in nodes {
        if heap.len() < n {
            heap.push(Reverse(node.stake()));
        } else if node.stake() >= heap.peek().unwrap().0 {
            heap.pop();
            heap.push(Reverse(node.stake()));
        }
    }
    heap.peek().map(|Reverse(stake)| nodes.iter().find(|node| node.stake() == *stake)).flatten()
}

fn main() {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "INFO");
    }
    solana_logger::setup();

    let matches = parse_matches();

    let config = Config {
        gossip_push_fanout: value_t_or_exit!(matches, "gossip_push_fanout", usize),
        gossip_active_set_size: value_t_or_exit!(matches, "gossip_push_active_set_entry_size", usize),
        gossip_iterations: value_t_or_exit!(matches, "gossip_iterations", usize),
        accounts_from_file: matches.is_present("accounts_from_yaml"),
        origin_rank: value_t_or_exit!(matches, "origin_rank", usize),
    };

    // check if we want to write keys and stakes to a file
    let account_file = matches.value_of("account_file").unwrap_or_default();

    // check if we want to read in pubkeys/stakes from a file
    let nodes = if config.accounts_from_file {
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

    info!("Using the following nodes and stakes");
    let (mut nodes, _): (Vec<_>, Vec<_>) = nodes
        .into_iter()
        .map(|(node, sender)| {
            info!("pubkey, stake: {:?}, {}", node.pubkey(), node.stake());
            let pubkey = node.pubkey();
            (node, (pubkey, sender))
        })
        .unzip();

    if nodes.len() < config.origin_rank {
        panic!("ERROR: origin_rank larger than number of simulation nodes. \
            nodes.len(): {}, origin_rank: {}", nodes.len(), config.origin_rank);
    }

    // TODO: remove unstaked here?!
    //get all of the stakes here. map node pubkey => stake
    //this includes unstaked nodes! so i guess Behzad's todo wants to remove unstaked
    let stakes: HashMap<Pubkey, /*stake:*/ u64> = nodes
        .iter()
        .map(|node| (node.pubkey(), node.stake()))
        .collect();
    
    //collect vector of nodes
    info!("Simulating Gossip and setting active sets. Please wait.....");
    let _res = run_gossip(&mut nodes, &stakes, config.gossip_active_set_size).unwrap();
    info!("Simulation Complete!");


    let node_map: HashMap<Pubkey, &Node> = nodes
        .iter()
        .map(|node| (node.pubkey(), node))
        .collect();

    let mut cluster: Cluster = Cluster::new(config.gossip_push_fanout);

    let origin_node = find_nth_largest_node(config.origin_rank, &nodes).unwrap();
    let origin_pubkey = &origin_node.pubkey();

    info!("ORIGIN: {:?}", origin_pubkey);
    let mut number_of_poor_coverage_runs: usize = 0;
    let poor_coverage_threshold: f64 = 0.95;
    let mut stats = GossipStats::default();
    for i in 0..config.gossip_iterations {
        info!("MST ITERATION: {}", i);
        info!("Calculating the MST for origin: {:?}", origin_pubkey);
        cluster.new_mst(origin_pubkey, &stakes, &node_map);
        info!("Calculation Complete. Printing results...");
        cluster.print_hops();
        // cluster.print_node_orders();

        info!("Origin Node: {:?}", origin_pubkey);
        cluster.print_mst();
        // cluster.print_prunes();

        let (coverage, stranded_nodes) = cluster.coverage(&stakes);
        info!("For origin {:?}, the cluster coverage is: {:.6}", origin_pubkey, coverage);
        info!("{} nodes are stranded", stranded_nodes);
        if stranded_nodes > 0 {
            cluster.stranded_nodes(&stakes);
        }
        if coverage < poor_coverage_threshold {
            warn!("WARNING: poor coverage for origin: {:?}, {}", origin_pubkey, coverage);
            number_of_poor_coverage_runs += 1;
        }
      
        stats.insert_coverage(coverage);
        stats.insert_hops_stat(
            HopsStat::new(
                cluster.get_distances()
            )
        );

        if log::log_enabled!(Level::Debug) {
            cluster.print_pushes();
        }

        // let _out = cluster.write_adjacency_list_to_file("../graph-viz/adjacency_list_pre.txt");
        cluster.prune_connections(origin_pubkey, &node_map, &stakes);
        info!("################################################################");
    }

    stats.calculate_coverage_stats();
    stats.print_coverage_stats();
    stats.print_hops_stats();


}

#[cfg(test)]
mod tests {
    use {
        // super::*,
        rand::SeedableRng, rand_chacha::ChaChaRng, std::iter::repeat_with,
        rand::Rng,
        solana_sdk::{pubkey::Pubkey},
        std::{
            collections::{BinaryHeap},
            time::Instant,
            cmp::Reverse,
        },
        solana_sdk::native_token::LAMPORTS_PER_SOL,
        log::info,
    };

    pub struct Node {
        pub pubkey: Pubkey,
        pub stake: u64,
    }

    fn create_nodes(stakes: Vec<u64>) -> Vec<Node> {
        let mut nodes = Vec::new();
    
        for stake in stakes {
            let pubkey = Pubkey::new_unique();
            let node = Node {
                pubkey,
                stake,
            };
            nodes.push(node);
        }
    
        nodes
    }
    
    fn find_nth_largest_node(n: usize, nodes: &[Node]) -> Option<&Node> {
        let mut heap = BinaryHeap::new();
        for node in nodes {
            if heap.len() < n {
                heap.push(Reverse(node.stake));
            } else if node.stake >= heap.peek().unwrap().0 {
                heap.pop();
                heap.push(Reverse(node.stake));
            }
        }
        heap.peek().map(|Reverse(stake)| nodes.iter().find(|node| node.stake == *stake)).flatten()
    }
    


    #[test]
    fn test_nth_largest() {
        let stakes: Vec<u64> = vec![10, 123, 67, 18, 29, 567, 12, 5, 875, 234, 12, 5, 76, 0, 12354, 985];
        let ranks: Vec<usize> = vec![5, 10, 12, 1, 6, 2, 9, 16];
        let res: Vec<u64> = vec![234, 18, 12, 12354, 123, 985, 29, 0];

        let nodes = create_nodes(stakes);

        for (index, r) in ranks.iter().enumerate() {
            let origin_node = find_nth_largest_node(*r, &nodes[..]).unwrap();
            let stake = origin_node.stake;
            assert_eq!(stake, res[index]);
        }
    }



}