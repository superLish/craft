use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use std::collections::HashMap;
use crate::network::NodeId;
use crate::network::connection::Connection;
use crate::config::Config;
use parity_crypto::publickey::{Generator, KeyPair, Public, Random, recover, Secret, sign, ecdh, ecies};
use ethereum_types::H512;


pub struct Host {
    nodeid: NodeId,
    keypair: KeyPair,
    ready_sessions: HashMap<NodeId, Connection>,

}

impl Host {
    pub fn new(config: &Config) -> Self {
        let secret = Secret::copy_from_str(config.secret.as_str()).unwrap();
        let keypair = KeyPair::from_secret(secret).unwrap();
        let nodeid = keypair.public().clone();
        info!("keypair: {}, nodeid: {:?}", keypair, nodeid);
        
        Host {
            nodeid,
            keypair,
            ready_sessions: HashMap::new(),
        }
    }

    /// 启动网络服务， 1. 开启监听；
    pub async fn start(&self, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(config.listen_addr.as_str()).await?;
        info!("listening on {}", config.listen_addr);
        loop {
            let (mut socket, addr) = listener.accept().await?;
            info!("accept tcp connection {}", addr);

            tokio::spawn( async move {
                let mut buf = [0; 1024];

                loop {
                    let n = match socket.read(&mut buf).await {
                        Ok(n) if n == 0 => {
                            info!("read 0, connection end.");
                            return;
                        },
                        Ok(n) => {
                            info!("read {} bytes from {}", n, addr);
                            n
                        },
                        Err(e) => {
                            error!("read failure from {}, {}", addr, e);
                            return;
                        }
                    };

                    if let Err(e) = socket.write_all(&buf[0..n]).await {
                        error!("failed to write to {}, {}", addr, e);
                        return;
                    }
                    info!("write bytes to {}", addr);
                }
            });
        }


        Ok(())
    }

    /// 主动连接到种子节点
    pub async fn connect_seed(&self, seed: &str) -> Result<(), Box<dyn std::error::Error>> {
        info!("prepare to connect to seed {}", seed);
        let mut stream = TcpStream::connect(seed).await?;
        info!("connected to {}, prepare to estabilsh session.", seed);



        Ok(())
    }

    /// 定时任务，待完成
    pub async fn timer_task(&self) {
        info!("timer task, wait to impl ......");
    }

}