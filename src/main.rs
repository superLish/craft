#[macro_use]
extern crate log;

mod network;
mod crypto;
mod config;
use yaml_rust::Yaml;
use std::env;
use std::fs::File;
use std::io::Read;
use crate::config::Config;
use crate::network::{ServerEvent, NetPacket};
use network::enode_str_parse;
use tokio::time::{sleep, Duration};

fn main() {
    simple_logger::SimpleLogger::new().with_level(log::LevelFilter::Info).init().unwrap();
    let name = env!("CARGO_PKG_NAME");
    let version = env!("CARGO_PKG_VERSION");
    info!("{}-{}.", name, version);

    let matches = clap::App::new(name)
        .version(version)
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(clap::Arg::with_name("config")
            .help("config")
            .short("c")
            .long("config")
            .value_name("path"))
        .subcommand(clap::SubCommand::with_name("debug")
            .about("use for debug project."))
        .subcommand(clap::SubCommand::with_name("generate_key")
            .about("generate keypair."))
        .get_matches();

    if let Some(_) = matches.subcommand_matches("generate_key") {
        info!("command for generate keypair:");
        let _ = crypto::generate_keypair();

        return;
    }

    // 读取配置文件中的配置
    let mut config = config::Config::default();
    if let Some(path) = matches.value_of("config") {
        let mut file = File::open(path).unwrap();
        let mut s = String::new();
        file.read_to_string(&mut s).unwrap();
        let cfgs = yaml_rust::YamlLoader::load_from_str(&s).unwrap();
        info!("{:?}", cfgs);
        for cfg in cfgs {
            match cfg {
                Yaml::Hash(hash) => {
                    if let Some(secret) = hash.get(&Yaml::String("secret".to_string())) {
                        let privkey = secret.clone().into_string().unwrap();
                        config.secret = privkey;
                    }
    
                    if let Some(local) = hash.get(&Yaml::String("local".to_string())) {
                        let listen = local.clone().into_string().unwrap();
                        config.listen_addr = listen;
                    }

                    if let Some(remote) = hash.get(&Yaml::String("remote".to_string())) {
                        let seed = remote.clone().into_string().unwrap();
                        config.seed = Some(seed);
                    }
                },  
                _ => {
                    error!("invalid config, please check config file.");
                    return;
                },
            }  
        }          
    }
    info!("config: {:?}", config);


    if let Some(_) = matches.subcommand_matches("debug") {
        // default config for debug.
        info!("debug for project.");

        let keypair = crypto::generate_keypair_from_secret_str(config.secret.as_str()).unwrap();
        info!("keypair: {}", keypair);
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_io()
        .enable_time().build().unwrap();

    runtime.block_on( async {
        start_client(config).await;
    });

    info!("craft end.");
}


async fn start_client(config: Config) {
    let seed = config.seed.clone();
    let (tx_net_event, mut rx_net_event) = tokio::sync::mpsc::channel(1024);
    let (tx_server_event, rx_server_event) = tokio::sync::mpsc::channel(1024);

    let recv_task = async {
        while let Some(event) = rx_net_event.recv().await {
            info!("client recv {:?}", event);
        }
    };

    let tx_server_event2 = tx_server_event.clone();
    tokio::spawn( async move {
        network::start_network(config, tx_server_event2, rx_server_event, tx_net_event.clone()).await;
    });

    let send_task = async {
        sleep(Duration::from_millis(10)).await;
        info!("send ServerEvent::Start to server, start listen service.");
        tx_server_event.send(ServerEvent::Start).await;
        if let Some(ref seed) = seed {
            sleep(Duration::from_millis(1000)).await;
            tx_server_event.send(ServerEvent::ActiveConnect(seed.clone())).await;
        }
    };

    let test_task = async {
        if let Some(ref seed) = seed {
            sleep(Duration::from_millis(10*1000)).await;
            let (nodeid, addr) = enode_str_parse(seed).unwrap();
            let data = vec![0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9];
            tx_server_event.send(ServerEvent::Send(NetPacket::new(nodeid, &data))).await;
        }
    };

    tokio::join!(recv_task, send_task, test_task);
}


// let task1 = async {
//     info!("task1");
// };
//
// let task2 = async {
//     info!("task2");
// };
//
// let main_task = async {
//     // loop {
//         tokio::select! {
//             _ = task1 => {
//                 info!("tast 1 exec.");
//             }
//             _ = task2 => {
//                 info!("task 2 exec.");
//             }
//         }
//     // }
// };
