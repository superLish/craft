//! 调试程序用的

#[macro_use]
extern crate log;

use tokio::net::TcpStream;
use tokio::prelude::*;
use tokio::time::Duration;

fn main() {
    simple_logger::SimpleLogger::new().with_level(log::LevelFilter::Info).init().unwrap();
    let name = env!("CARGO_PKG_NAME");
    let version = env!("CARGO_PKG_VERSION");

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
        .get_matches();

    if let Some(_) = matches.subcommand_matches("debug") {
        // default config for debug.
        info!("debug for project.");
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_io()
        .enable_time().build().unwrap();

    runtime.block_on( async {
        info!("run client.");
        if let Err(e) = run_tcp_client().await {
            error!("{:?}", e);
        }
    });

}

async fn run_tcp_client() -> Result<(), Box<dyn std::error::Error>> {
    info!("prepare to connect to 127.0.0.1:50000");
    let mut stream = TcpStream::connect("127.0.0.1:50000").await?;
    info!("connected to 127.0.0.1:50000");

    stream.write_all(b"hello world!").await?;

    let mut buffer = [0;1024];
    let n = stream.read(&mut buffer).await?;
    info!("read bytes: {:?}", &buffer[..n]);

    tokio::time::sleep(Duration::new(60, 0)).await;
    Ok(())
}