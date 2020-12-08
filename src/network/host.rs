use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use std::collections::{HashMap, HashSet};
use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::str::FromStr;
use crate::network::NodeId;
use crate::config::Config;
use crate::network::session::{Session, SessionReadResult};
// use crate::network::connection::ReadResult;
use parity_crypto::publickey::{Generator, KeyPair, Public, Random, recover, Secret, sign, ecdh, ecies};
use ethereum_types::H512;
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::Mutex;
use slab::Slab;

type ShareSession = Arc<Mutex<Session>>;

pub struct Host {
    nodeid: NodeId,
    keypair: KeyPair,
    ready_sessions: Arc<RwLock<HashMap<NodeId, ShareSession>>>,     // 就绪状态的会话
    pending_sessions: Arc<RwLock<Slab<ShareSession>>>,               // 未就绪状态的会话
    version: u8,
}

impl Host {
    pub fn new(config: &Config, version: u8) -> Self {
        let secret = Secret::copy_from_str(config.secret.as_str()).unwrap();
        let keypair = KeyPair::from_secret(secret).unwrap();
        let nodeid = keypair.public().clone();
        info!("keypair: {}, nodeid: {:?}", keypair, nodeid);
        
        Host {
            nodeid,
            keypair,
            ready_sessions: Arc::new(RwLock::new(HashMap::new())),
            pending_sessions: Arc::new(RwLock::new(Slab::new())),
            version
        }
    }

    async fn process_new_connect(share_session: ShareSession, ready_sessions: Arc<RwLock<HashMap<NodeId, ShareSession>>>, pending_sessions: Arc<RwLock<Slab<ShareSession>>>) {
        let arc_session = share_session.clone();
        let key = pending_sessions.write().insert(share_session);
        info!("create session, before session reading.");
        let session_reading = arc_session.lock().await.reading().await;       // fixme: 错误处理
        match session_reading {
            SessionReadResult::Packet(data) => {
                // fixme: 这个地方不会返回，应该在某个地方将消息发送到上层。
                info!("host read {:?}", data);
            },
            SessionReadResult::Hub => {
                info!("host read connection disconnected, remove.");
                if let Some(nodeid) = arc_session.lock().await.remote() {
                    info!("ready_sessions before remove {}", ready_sessions.read().len());
                    ready_sessions.write().remove(nodeid);
                    info!("ready_sessions after remove {}", ready_sessions.read().len());
                } else {
                    info!("pending_sessions before remove key<{}>, {}", key, pending_sessions.read().len());
                    pending_sessions.write().remove(key);
                    info!("pending_sessions after remove key<{}>, {}", key, pending_sessions.read().len());
                }
            },
            SessionReadResult::Error(e) => {
                error!("host read error {}", e);
                if let Some(nodeid) = arc_session.lock().await.remote() {
                    info!("ready_sessions before remove {}", ready_sessions.read().len());
                    ready_sessions.write().remove(nodeid);
                    info!("ready_sessions after remove {}", ready_sessions.read().len());
                } else {
                    info!("pending_sessions before remove key<{}>, {}", key, pending_sessions.read().len());
                    pending_sessions.write().remove(key);
                    info!("pending_sessions after remove key<{}>, {}", key, pending_sessions.read().len());
                }
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

            let mut session = Session::new(socket, None, self.version);
            let share_session = Arc::new(Mutex::new(session));

            tokio::spawn( async move {
                Host::process_new_connect(share_session, ready_sessions, pending_sessions).await;
            });
        }

        Ok(())
    }

    /// 主动连接到种子节点
    pub async fn connect_seed(&self, seed: &str) -> Result<(), Box<dyn std::error::Error>> {
        info!("prepare to connect to seed {}", seed);
        let (nodeid, addr) = enode_str_parse(seed)?;
        let stream = TcpStream::connect(addr).await?;
        info!("connected to {}, prepare to estabilsh session.", seed);

        let ready_sessions = self.ready_sessions.clone();
        let pending_sessions = self.pending_sessions.clone();

        let session = Session::new(stream, Some(nodeid), self.version);
        let share_session = Arc::new(Mutex::new(session));
        let arc_session = share_session.clone();
        tokio::spawn( async move {
            Host::process_new_connect(share_session, ready_sessions, pending_sessions).await;
        });

        arc_session.lock().await.write_auth().await?;

        Ok(())
    }

    /// 定时任务，待完成
    pub async fn timer_task(&self) {
        info!("timer task, wait to impl ......");
        // fixme: KAD节点发现服务待实现。
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