use {
    clap::{crate_description, crate_name, App, Arg, ArgMatches, value_t_or_exit},
    log::{error, info, debug, warn, Level},
    gossip_sim::{
        API_MAINNET_BETA,
        Error,
        gossip::{
            make_gossip_cluster_from_rpc, 
            make_gossip_cluster_from_map,
            Node,
            Cluster,
            Config,
            Testing,
            StepSize,
        },
        gossip_stats::{
            GossipStats,
            GossipStatsCollection,
        },
        influx_db::{
            InfluxDB,
            InfluxDataPoint,
            InfluxThread,
        },
    },
    solana_client::rpc_client::RpcClient,
    solana_sdk::pubkey::Pubkey,
    std::{
        fs::{File}, 
        path::Path, 
        collections::{HashMap, BinaryHeap, VecDeque},
        process::exit,
        cmp::Reverse,
        env,
        rc::Rc,
        cell::RefCell,
        sync::{Arc, Mutex},
        thread,
    },
    rand::rngs::StdRng,
    rand::SeedableRng,
    rayon::prelude::*,
    dotenv::dotenv,
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
            Arg::with_name("remove_zero_staked_nodes")
                .long("filter-zero-staked-nodes")
                .short('f')
                .takes_value(false)
                .help("Filter out all zero-staked nodes"),
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
        .arg(
            Arg::with_name("active_set_rotation_probability")
                .long("rotation-probability")
                .short('p')
                .takes_value(true)
                .default_value(".013333") // avg. one rotation for all nodes per 75 gossip rounds (1/75)
                .validator(|s| match s.parse::<f64>() {
                    Ok(n) if n >= 0.0 && n <= 1.0 => Ok(()),
                    _ => Err(String::from("active_set_rotation_probability must be between 0 and 1")),
                })
                .help("After each round of gossip, rotate a node's active set with a set probability 0 <= p <= 1"),
        )
        .arg(
            Arg::with_name("min_ingress_nodes")
                .long("min-ingress-nodes")
                .takes_value(true)
                .default_value("2")
                .help("Minimum number of incoming peers a node must keep"),
        )
        .arg(
            Arg::with_name("prune_stake_threshold")
                .long("stake-threshold")
                .takes_value(true)
                .default_value(".15")
                .validator(|s| match s.parse::<f64>() {
                    Ok(n) if n >= 0.0 && n <= 1.0 => Ok(()),
                    _ => Err(String::from("prune_stake_threshold must be between 0 and 1")),
                })
                .help("Ensure a node is connected to a minimum stake of prune_stake_threshold*node.stake()"),
        )
        .arg(
            Arg::with_name("num_buckets_for_stranded_node_hist")
                .long("num-buckets")
                .takes_value(true)
                .default_value("10")
                .help("Number of buckets for the stranded node histogram. see gossip_stats.rs"),
        )
        .arg(
                Arg::with_name("test_type")
                .long("test-type")
                .takes_value(true)
                .validator(validate_testing)
                .requires("num_simulations")
                .requires("step_size")
                .help("Type of test to run.
                    active-set-size
                    push-fanout
                    min-ingress-nodes
                    min-stake-threshold
                    origin-rank
                    [default: no-test]
                "),
        )
        .arg(
            Arg::with_name("num_simulations")
                .long("num-simulations")
                .takes_value(true)
                .default_value("1")
                .help("Number of simulations to run. [default: 1]"),
        )
        .arg(
            Arg::with_name("step_size")
                .long("step-size")
                .takes_value(true)
                .default_value("1")
                .requires("test_type")
                .help("Size of step for test_type. [default: 1]"),
        )
        .arg(
            Arg::with_name("fail_nodes")
                .long("fail-nodes")
                .takes_value(false)
                .requires("when_to_fail")
                .help("Fail a certain percentage of nodes"),
        )
        .arg(
            Arg::with_name("fraction_to_fail")
                .long("fraction-to-fail")
                .takes_value(true)
                .default_value("0.1")
                .requires("fail_nodes")
                .help("Fail `fraction-to-fail` of total nodes in cluster"),
        )
        .arg(
            Arg::with_name("when_to_fail")
                .long("when-to-fail")
                .takes_value(true)
                .default_value("0")
                .requires("fail_nodes")
                .help("On what iteration should the nodes fail"),
        )
        .arg(
            Arg::with_name("warm_up_rounds")
                .long("warm-up-rounds")
                .takes_value(true)
                .default_value("200")
                .help("Number of gossip rounds to run before measuring statistics"),
        )
        .arg(
            Arg::with_name("influx")
                .long("influx")
                .takes_value(true)
                .default_value("l")
                .help("Influx for reporing metrics. i for internal-metrics, l for localhost"),
        )
        .get_matches()
}


fn load_influx_env_vars() -> Result<(), dotenv::Error> {
    dotenv().map(|_| ())
}

fn validate_testing(val: &str) -> Result<(), String> {
    val.parse::<Testing>()
        .map(|_| ())
        .map_err(|_| "Invalid test type".to_string())
}


pub fn initialize_gossip(
    // nodes: &[RwLock<Node>],
    nodes: &mut Vec<Node>,
    stakes: &HashMap<Pubkey, /*stake:*/ u64>,
    active_set_size: usize,
) -> Result<(), Error> {
    let rng = StdRng::from_entropy();
    
    nodes.par_iter_mut().for_each(|node| {
        let mut local_rng = rng.clone();
        node.initialize_gossip(&mut local_rng, stakes, active_set_size, false);
    });

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

fn run_simulation(
    config: &Config, 
    matches: &ArgMatches, 
    gossip_stats_collection: &mut GossipStatsCollection, 
    // influx_db: Option<&mut Rc<RefCell<InfluxDB>>>,
    datapoint_queue: &Arc<Mutex<VecDeque<InfluxDataPoint>>>,
    simulation_iteration: usize,
) {
    info!("##### SIMULATION ITERATION: {} #####", simulation_iteration);
    // check if we want to read in pubkeys/stakes from a file
    let nodes = if config.accounts_from_file {
        // READ ACCOUNTS FROM FILE
        if config.account_file.is_empty() {
            error!("Failed to pass in account file to read from with --accounts-from-yaml flag. need --acount-file <path>");
            exit(-1);
        }
        let path = Path::new(&config.account_file);
        let file = File::open(path).unwrap();

        info!("Reading {}", config.account_file);
        let accounts: HashMap<String, u64> = serde_yaml::from_reader(file).unwrap();
        info!("{} accounts read in", accounts.len());
        let nodes = make_gossip_cluster_from_map(&accounts, config.filter_zero_staked_nodes);
        nodes

    } else {
        let json_rpc_url =
            gossip_sim::get_json_rpc_url(matches.value_of("json_rpc_url").unwrap_or_default());
        info!("json_rpc_url: {}", json_rpc_url);
        let rpc_client = RpcClient::new(json_rpc_url);
        let nodes = make_gossip_cluster_from_rpc(&rpc_client, config.filter_zero_staked_nodes);
        nodes
    }.unwrap();

    info!("{:#?}", config);

    debug!("Using the following nodes and stakes");
    let (mut nodes, _): (Vec<_>, Vec<_>) = nodes
        .into_iter()
        .map(|(node, sender)| {
            debug!("pubkey, stake: {:?}, {}", node.pubkey(), node.stake());
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
    let _res = initialize_gossip(&mut nodes, &stakes, config.gossip_active_set_size).unwrap();
    info!("Simulation Complete!");

    let mut cluster: Cluster = Cluster::new(config.gossip_push_fanout);

    let origin_node = find_nth_largest_node(config.origin_rank, &nodes).unwrap();
    let origin_pubkey = &origin_node.pubkey();

    info!("ORIGIN: {:?}", origin_pubkey);
    let mut _number_of_poor_coverage_runs: usize = 0;
    let poor_coverage_threshold: f64 = 0.95;
    let mut stats = GossipStats::default();
    stats.set_simulation_parameters(config);
    stats.set_origin(*origin_pubkey);
    info!("Calculating the MSTs for origin: {:?}, stake: {}", origin_pubkey, stakes.get(origin_pubkey).unwrap());
    for gossip_iteration in 0..config.gossip_iterations {
        if gossip_iteration % 10 == 0 {
            info!("MST ITERATION: {}", gossip_iteration);
        }
            
        if config.fail_nodes && gossip_iteration == config.when_to_fail {
            cluster.fail_nodes(config.fraction_to_fail, &mut nodes);
            stats.set_failed_nodes(cluster.get_failed_nodes());
        }
        
        {
            let node_map: HashMap<Pubkey, &Node> = nodes
                .iter()
                .map(|node| (node.pubkey(), node))
                .collect();
            cluster.run_gossip(origin_pubkey, &stakes, &node_map);
        }

        cluster.consume_messages(origin_pubkey, &mut nodes);
        cluster.send_prunes(*origin_pubkey, &mut nodes, config.prune_stake_threshold, config.min_ingress_nodes, &stakes);

        {
            let node_map: HashMap<Pubkey, &Node> = nodes
                .iter()
                .map(|node| (node.pubkey(), node))
                .collect();
            cluster.prune_connections(&node_map, &stakes);
        }

        let mut rng = rand::thread_rng();
        cluster.chance_to_rotate(&mut rng, &mut nodes, config.gossip_active_set_size, &stakes, config.probability_of_rotation, &mut StdRng::from_entropy());

        // wait until after set number of warmup rounds to begin calculating gossip stats
        if gossip_iteration >= config.warm_up_rounds {
            // don't care about gossip_iteration 0->warm_up_rounds.
            // so create new variable that we are going to use for data-recorded iterations
            // this is essentially our steady state  
            let steady_state_iteration = gossip_iteration - config.warm_up_rounds;
            let (coverage, stranded_nodes) = cluster.coverage(&stakes);
            debug!("For origin {:?}, the cluster coverage is: {:.6}", origin_pubkey, coverage);
            debug!("{} nodes are stranded out of {} nodes", stranded_nodes, nodes.len());
            if coverage < poor_coverage_threshold {
                warn!("WARNING: poor coverage for origin: {:?}, {}", origin_pubkey, coverage);
                _number_of_poor_coverage_runs += 1;
            }

            stats.insert_coverage(coverage);
            stats.insert_hops_stat(cluster.get_distances());
            
            match cluster.relative_message_redundancy() {
                Ok(result) => {
                    stats.insert_rmr(result);
                    // if let Some(ref db) = influx_db {
                    //     db.borrow_mut().create_data_point(
                    //         result,
                    //         "rmr".to_string(), 
                    //         steady_state_iteration,
                    //         simulation_iteration
                    //     );
                    // }
                },
                Err(_) => error!("Network RMR error. # of nodes is 1."),
            }

            stats.insert_stranded_nodes(
                &cluster.stranded_nodes(), 
                &stakes
            );
            
            if log::log_enabled!(Level::Debug) {
                cluster.print_pushes();
            }

            stats.calculate_outbound_branching_factor(cluster.get_pushes());

            let mut datapoint = InfluxDataPoint::default();
            datapoint.create_data_point(
                coverage, 
                "coverage".to_string()
            );
            datapoint.append_timestamp();

            datapoint_queue.lock().unwrap().push_back(datapoint);

            // if let Some(ref db) = influx_db {
            //     db.borrow_mut().create_data_point(
            //         coverage,
            //         "coverage".to_string(), 
            //         steady_state_iteration,
            //         simulation_iteration
            //     );

            //     db.borrow_mut().create_hops_stat_point(
            //         stats.get_hops_stat_by_iteration(steady_state_iteration),
            //         steady_state_iteration,
            //         simulation_iteration
            //     );

            //     db.borrow_mut().create_stranded_node_stat_point(
            //         stats.get_stranded_node_stats_by_iteration(steady_state_iteration),
            //         steady_state_iteration,
            //         simulation_iteration
            //     );

            //     db.borrow_mut().create_data_point(
            //         stats.get_outbound_branching_factor_by_index(steady_state_iteration),
            //         "branching_factor".to_string(), 
            //         steady_state_iteration,
            //         simulation_iteration
            //     );
            //     db.borrow().send_data_points();
            // }
        }
    }
    let mut datapoint = InfluxDataPoint::default();
    datapoint.set_last_datapoint();
    datapoint_queue.lock().unwrap().push_back(datapoint);

    if !stats.is_empty() {
        stats.run_all_calculations(config.num_buckets_for_stranded_node_hist);
        gossip_stats_collection.push(stats.clone());
    }
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
        account_file: matches.value_of("account_file").unwrap_or_default(),
        origin_rank: value_t_or_exit!(matches, "origin_rank", usize),
        probability_of_rotation: value_t_or_exit!(matches, "active_set_rotation_probability", f64),
        prune_stake_threshold: value_t_or_exit!(matches, "prune_stake_threshold", f64), 
        min_ingress_nodes: value_t_or_exit!(matches, "min_ingress_nodes", usize),
        fail_nodes: matches.is_present("fail_nodes"),
        fraction_to_fail: value_t_or_exit!(matches, "fraction_to_fail", f64), 
        when_to_fail: value_t_or_exit!(matches, "when_to_fail", usize),
        filter_zero_staked_nodes: matches.is_present("remove_zero_staked_nodes"),
        num_buckets_for_stranded_node_hist: value_t_or_exit!(matches, "num_buckets_for_stranded_node_hist", u64),
        test_type: matches
                    .value_of("test_type")
                    .map(|val| val.parse::<Testing>()
                        .unwrap_or_else(|_| Testing::NoTest)
                    )
                    .unwrap_or(Testing::NoTest),
        num_simulations: matches
                    .value_of("num_simulations")
                    .map(|val| val.parse::<usize>().unwrap_or_else(|_| {
                        eprintln!("Invalid num_simulations value");
                        exit(1);
                    }))
                    .unwrap(),
        step_size: matches
                    .value_of("step_size")
                    .map(|val| {
                        val.parse::<usize>()
                            .map(StepSize::Integer)
                            .unwrap_or_else(|_| {
                                val.parse::<f64>()
                                    .map(StepSize::Float)
                                    .unwrap_or_else(|_| {
                                        eprintln!("Invalid step_size value");
                                        exit(1);
                                    })
                            })
                    })
                    .unwrap(),
        warm_up_rounds: value_t_or_exit!(matches, "warm_up_rounds", usize),
    };

    if config.gossip_iterations <= config.warm_up_rounds {
        warn!("WARNING: Gossip Iterations ({}) <= Warm Up Rounds ({}). No stats will be recorded....", 
            config.gossip_iterations, 
            config.warm_up_rounds);
    }

    let influx_type = matches.value_of("influx").unwrap_or_default().to_string().clone();
    

    let datapoint_queue: Arc<Mutex<VecDeque<InfluxDataPoint>>> = Arc::new(Mutex::new(VecDeque::new()));
    let influx_db_queue = datapoint_queue.clone();

    if let Err(err) = load_influx_env_vars() {
        error!("Failed to load environment variables: {}", err);
        return;
    }
    let influx_thread = thread::spawn(move || {
        InfluxThread::start(
            gossip_sim::get_influx_url(
                influx_type.as_str()
            ),
            env::var("GOSSIP_SIM_INFLUX_USERNAME")
                .unwrap_or_else(|_| {
                    error!("GOSSIP_SIM_INFLUX_USERNAME is not set");
                    exit(1);
            }),
            env::var("GOSSIP_SIM_INFLUX_PASSWORD")
                .unwrap_or_else(|_| {
                    error!("GOSSIP_SIM_INFLUX_PASSWORD is not set");
                    exit(1);
            }),
            env::var("GOSSIP_SIM_INFLUX_DATABASE")
                .unwrap_or_else(|_| {
                    error!("GOSSIP_SIM_INFLUX_DATABASE is not set");
                    exit(1);
            }),
            influx_db_queue
        )
    });



    // check if we are going to push data to influx
    // // if --influx set, we are pushing to influx
    // let mut influx_db: Option<Rc<RefCell<InfluxDB>>> = if let Some(influx_url) = matches.value_of("influx") {
    //     if let Err(err) = load_influx_env_vars() {
    //         error!("Failed to load environment variables: {}", err);
    //         return;
    //     }
    
    //     Some(Rc::new(RefCell::new(InfluxDB::new(
    //         gossip_sim::get_influx_url(
    //             influx_url
    //         ),
    //         env::var("GOSSIP_SIM_INFLUX_USERNAME")
    //             .unwrap_or_else(|_| {
    //                 error!("GOSSIP_SIM_INFLUX_USERNAME is not set");
    //                 exit(1);
    //             }),
    //             env::var("GOSSIP_SIM_INFLUX_PASSWORD")
    //             .unwrap_or_else(|_| {
    //                 error!("GOSSIP_SIM_INFLUX_PASSWORD is not set");
    //                 exit(1);
    //             }),
    //             env::var("GOSSIP_SIM_INFLUX_DATABASE")
    //             .unwrap_or_else(|_| {
    //                 error!("GOSSIP_SIM_INFLUX_DATABASE is not set");
    //                 exit(1);
    //             })
    //     ).unwrap())))
    // } else {
    //     None
    // };

    let mut gossip_stats_collection = GossipStatsCollection::default();
    gossip_stats_collection.set_number_of_simulations(config.num_simulations);

    match config.test_type {
        Testing::ActiveSetSize => {
            let initial_active_set_size = config.gossip_active_set_size;

            for i in 0..config.num_simulations {
                let step_size: usize = config.step_size.into();
                let active_set_size = initial_active_set_size + (i * step_size);

                // Update the active_set_size in the config for each experiment
                let mut config = config.clone();
                config.gossip_active_set_size = active_set_size;
        
                // Run the experiment with the updated config
                run_simulation(&config, &matches, &mut gossip_stats_collection, &datapoint_queue, i);
            }
        }
        Testing::PushFanout => {
            let step_size: usize = config.step_size.into();
            let initial_push_fanout = config.gossip_push_fanout;

            for i in 0..config.num_simulations {
                let push_fanout = initial_push_fanout + (i * step_size);

                // Update the active_set_size in the config for each experiment
                let mut config = config.clone();
                config.gossip_push_fanout = push_fanout;
        
                // Run the experiment with the updated config
                run_simulation(&config, &matches, &mut gossip_stats_collection, &datapoint_queue, i);
            }

        }
        Testing::MinIngressNodes => {
            let step_size: usize = config.step_size.into();
            let initial_min_ingress_nodes = config.min_ingress_nodes;

            for i in 0..config.num_simulations {
                let min_ingress_nodes = initial_min_ingress_nodes + (i * step_size);

                // Update the active_set_size in the config for each experiment
                let mut config = config.clone();
                config.min_ingress_nodes = min_ingress_nodes;
        
                // Run the experiment with the updated config
                run_simulation(&config, &matches, &mut gossip_stats_collection, &datapoint_queue, i);
            }
        }
        Testing::MinStakeThreshold => {
            let step_size: f64 = config.step_size.into();
            let initial_prune_stake_threshold = config.prune_stake_threshold;

            for i in 0..config.num_simulations {
                let prune_stake_threshold = initial_prune_stake_threshold + (i as f64 * step_size);

                // Update the active_set_size in the config for each experiment
                let mut config = config.clone();
                config.prune_stake_threshold = prune_stake_threshold;
        
                // Run the experiment with the updated config
                run_simulation(&config, &matches, &mut gossip_stats_collection, &datapoint_queue, i);
            }
        }
        Testing::OriginRank => {
            let step_size: usize = config.step_size.into();
            let initial_origin_rank = config.origin_rank;

            for i in 0..config.num_simulations {
                let origin_rank = initial_origin_rank + (i * step_size);

                // Update the active_set_size in the config for each experiment
                let mut config = config.clone();
                config.origin_rank = origin_rank;
        
                // Run the experiment with the updated config
                run_simulation(&config, &matches, &mut gossip_stats_collection, &datapoint_queue, i);
            }
        }
        Testing::NoTest => {
            for i in 0..config.num_simulations {
                run_simulation(&config, &matches, &mut gossip_stats_collection, &datapoint_queue, i);
            }
        }
    }

    // if !gossip_stats_collection.is_empty() {
    //     gossip_stats_collection.print_all(config.gossip_iterations, config.warm_up_rounds, config.test_type);
    // } else {
    //     warn!("WARNING: Gossip Stats Collection is empty. Is `Iterations` <= `warm-up-rounds`?");
    // }

    let _ = influx_thread.join();
}

#[cfg(test)]
mod tests {
    use {
        solana_sdk::{pubkey::Pubkey},
        std::{
            collections::{BinaryHeap, HashMap},
            cmp::Reverse,
            iter::repeat_with,
        },
        gossip_sim::gossip::{
            Node,
            Cluster,
            make_gossip_cluster_for_tests,
        },
        solana_sdk::native_token::LAMPORTS_PER_SOL,
        rand::SeedableRng,
        rand_chacha::ChaChaRng,
        rand::Rng,
        rand::rngs::StdRng,
    };

    pub struct TestNode {
        pub pubkey: Pubkey,
        pub stake: u64,
    }

    pub fn run_gossip(
        rng: &mut ChaChaRng,
        nodes: &mut Vec<Node>,
        stakes: &HashMap<Pubkey, /*stake:*/ u64>,
        active_set_size: usize,
    ) {
        for node in nodes {
            node.initialize_gossip(rng, stakes, active_set_size, true);
        }
    }

    pub fn get_bucket(stake: &u64) -> u64 {
        let stake = stake / LAMPORTS_PER_SOL;
        // say stake is high. few leading zeros. so bucket is high.
        let bucket = u64::BITS - stake.leading_zeros();
        // get min of 24 and bucket. higher numbered buckets are higher stake. low buckets low stake
        let bucket = (bucket as usize).min(25 - 1);
        bucket as u64
    }

    fn create_nodes(stakes: Vec<u64>) -> Vec<TestNode> {
        let mut nodes = Vec::new();
    
        for stake in stakes {
            let pubkey = Pubkey::new_unique();
            let node = TestNode {
                pubkey,
                stake,
            };
            nodes.push(node);
        }
    
        nodes
    }
    
    fn find_nth_largest_node(n: usize, nodes: &[TestNode]) -> Option<&TestNode> {
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

    #[test]
    fn test_pruning() {
        const PUSH_FANOUT: usize = 2;
        const ACTIVE_SET_SIZE: usize = 12;
        const PRUNE_STAKE_THRESHOLD: f64 = 0.15;
        const MIN_INGRESS_NODES: usize = 2;
        const CHANCE_TO_ROTATE: f64 = 0.2;
        const GOSSIP_ITERATIONS: usize = 21;

        let nodes: Vec<_> = repeat_with(Pubkey::new_unique).take(5).collect();
        const MAX_STAKE: u64 = (1 << 20) * LAMPORTS_PER_SOL;
        let mut rng = ChaChaRng::from_seed([189u8; 32]);
        let pubkey = Pubkey::new_unique();
        let stakes = repeat_with(|| rng.gen_range(1, MAX_STAKE));
        let mut stakes: HashMap<_, _> = nodes.iter().copied().zip(stakes).collect();
        stakes.insert(pubkey, rng.gen_range(1, MAX_STAKE));

        let res = make_gossip_cluster_for_tests(&stakes)
            .unwrap();

        let (mut nodes, _): (Vec<_>, Vec<_>) = res
            .into_iter()
            .map(|(node, sender)| {
                let pubkey = node.pubkey();
                (node, (pubkey, sender))
            })
            .unzip();

        // sort to maintain order across tests
        nodes.sort_by_key(|node| node.pubkey() );

        // setup gossip
        run_gossip(&mut rng, &mut nodes, &stakes, ACTIVE_SET_SIZE);

        let mut cluster: Cluster = Cluster::new(PUSH_FANOUT);
        let origin_pubkey = &pubkey; //just a temp origin selection

        // verify buckets
        let mut keys = stakes.keys().cloned().collect::<Vec<_>>();
        keys.sort_by_key(|&k| stakes.get(&k));

        let buckets: Vec<u64> = vec![15, 16, 19, 19, 20, 20];
        for (key, &bucket) in keys.iter().zip(buckets.iter()) {
            let current_stake = stakes.get(&key).unwrap();
            let bucket_from_stake = get_bucket(current_stake);
            assert_eq!(bucket, bucket_from_stake);
        }

        for i in 0..GOSSIP_ITERATIONS {
            {
                let node_map: HashMap<Pubkey, &Node> = nodes
                    .iter()
                    .map(|node| (node.pubkey(), node))
                    .collect();
                cluster.run_gossip(origin_pubkey, &stakes, &node_map);
            }
            // in this test all nodes should be visited
            assert_eq!(cluster.get_visited_len(), 6);

            // cluster.print_mst();
            cluster.print_node_orders();

            cluster.consume_messages(origin_pubkey, &mut nodes);
            cluster.send_prunes(*origin_pubkey, &mut nodes, PRUNE_STAKE_THRESHOLD, MIN_INGRESS_NODES, &stakes);
            let prunes = cluster.get_prunes();
            assert_eq!(prunes.len(), 6);
            for (pruner, prune) in prunes.iter() {
                if i <= 18 {
                    assert_eq!(prune.len(), 0);
                }
                // println!("pruner: {:?}", pruner);
                for (prunee, _) in prune.iter() {
                    // println!("prunee: {:?}", prunee);
                    if pruner == &nodes[2].pubkey() {               // 3 prunes M
                        assert_eq!(prunee, &nodes[0].pubkey());
                    } else if pruner == &nodes[0].pubkey() {        // M prunes H
                        assert_eq!(prunee, &nodes[1].pubkey()); 
                    } else if pruner == &nodes[4].pubkey() {        // J prunes P
                        assert_eq!(prunee, &nodes[3].pubkey());
                    }
                }
            }
            {
                let node_map: HashMap<Pubkey, &Node> = nodes
                    .iter()
                    .map(|node| (node.pubkey(), node))
                    .collect();
                cluster.prune_connections(&node_map, &stakes);
            }

            let seed = [42u8; 32];
            let mut rotate_seed_rng = StdRng::from_seed(seed);
            let mut rotate_seed_rng_2 = StdRng::from_seed(seed);
            cluster.chance_to_rotate(&mut rotate_seed_rng_2, &mut nodes, ACTIVE_SET_SIZE, &stakes, CHANCE_TO_ROTATE, &mut rotate_seed_rng);
        }
    }
}