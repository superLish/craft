#[macro_use]
extern crate log;

mod network;
mod crypto;
mod config;

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

    if let Some(_) = matches.subcommand_matches("debug") {
        // default config for debug.
        info!("debug for project.");

        let config = config::Config::default();
        let keypair = crypto::generate_keypair_from_secret_str(config.secret.as_str()).unwrap();
        info!("keypair: {}", keypair);
    }

    if let Some(_) = matches.subcommand_matches("generate_key") {
        info!("command for generate keypair:");
        let _ = crypto::generate_keypair();

        return;
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_io()
        .enable_time().build().unwrap();

    runtime.block_on( async {
        info!("tokio runtime.");
        let network = network::NetworkService::new();
        if let Err(e) = network.start().await {
            error!("{:?}", e);
        }
    });

    info!("craft end.");
}
