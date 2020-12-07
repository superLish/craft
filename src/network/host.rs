use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use std::collections::{HashMap, HashSet};
use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::str::FromStr;
use crate::network::NodeId;
use crate::config::Config;
use crate::network::session::Session;
use crate::network::connection::ReadResult;
use parity_crypto::publickey::{Generator, KeyPair, Public, Random, recover, Secret, sign, ecdh, ecies};
use ethereum_types::H512;
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::Mutex;

type ShareSession = Arc<Mutex<Session>>;

pub struct Host {
    nodeid: NodeId,
    keypair: KeyPair,
    ready_sessions: Arc<RwLock<HashMap<NodeId, ShareSession>>>,     // 就绪状态的会话
    pending_sessions: Arc<RwLock<Vec<ShareSession>>>,               // 未就绪状态的会话
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
            ready_sessions: Arc::new(RwLock::new(HashMap::new())),
            pending_sessions: Arc::new(RwLock::new(Vec::new())),
        }
    }

    async fn process_new_connect(socket: TcpStream, ready_sessions: Arc<RwLock<HashMap<NodeId, ShareSession>>>, pending_sessions: Arc<RwLock<Vec<ShareSession>>>) {
        let mut session = Session::new(socket, None);

        let share_session = Arc::new(Mutex::new(session));
        let arc_session = share_session.clone();
        pending_sessions.write().push(share_session);
        info!("create session, before session reading.");
        let session_reading = arc_session.lock().await.reading().await;       // fixme: 错误处理
        match session_reading {
            ReadResult::Packet(data) => {
                info!("host read {:?}", data);
            },
            ReadResult::Hub => {
                // fixme:
                info!("host read connection disconnected.");
            },
            ReadResult::Error(e) => {
                //fixme:
                error!("host read error {}", e);
            }
        }
        info!("after session reading.");


    }


    /// 启动网络服务， 1. 开启监听；
    pub async fn start(&self, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(config.listen_addr.as_str()).await?;
        info!("listening on {}", config.listen_addr);
        loop {
            let (socket, addr) = listener.accept().await?;
            info!("accept tcp connection {}", addr);
            let ready_sessions = self.ready_sessions.clone();
            let pending_sessions = self.pending_sessions.clone();
            tokio::spawn( async move {
                Host::process_new_connect(socket, ready_sessions, pending_sessions).await;
            });
/*
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
                }
            });
*/
        }

        Ok(())
    }

    /// 主动连接到种子节点
    pub async fn connect_seed(&self, seed: &str) -> Result<(), Box<dyn std::error::Error>> {
        info!("prepare to connect to seed {}", seed);
        let (nodeid, addr) = enode_str_parse(seed)?;
        let stream = TcpStream::connect(addr).await?;
        info!("connected to {}, prepare to estabilsh session.", seed);

        let session = Session::new(stream, Some(nodeid));
        let mut lock = self.pending_sessions.write();
        let share_session = Arc::new(Mutex::new(session));
        let arc_session = share_session.clone();
        lock.push(share_session);

        arc_session.lock().await.write_auth().await?;

        Ok(())
    }

    /// 定时任务，待完成
    pub async fn timer_task(&self) {
        info!("timer task, wait to impl ......");
    }

}

fn enode_str_parse(node: &str) -> Result<(NodeId, &str), Box<dyn std::error::Error>> {
    info!("node is {}", node);
    let pubkey = &node[8..136];
    info!("pubkey is {}", pubkey);
    let nodeid = H512::from_str(pubkey)?;
    let addr = &node[137..];
    info!("socket addr is {}", addr);
    
    Ok((nodeid, addr))
}