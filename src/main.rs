#[macro_use]
extern crate log;

mod network;
mod crypto;
mod config;
use yaml_rust::Yaml;
use std::env;
use std::fs::File;
use std::io::Read;

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
        network::start_network(config).await;
    });

    info!("craft end.");
}
