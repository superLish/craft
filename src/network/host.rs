use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use std::collections::{HashMap, HashSet};
use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::str::FromStr;
use crate::network::NodeId;
use crate::config::Config;
use crate::network::session::{Session, SessionReadResult};
use parity_crypto::publickey::{Generator, KeyPair, Public, Random, recover, Secret, sign, ecdh, ecies};
use ethereum_types::H512;
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Interval, Duration};
use slab::Slab;
use std::io::Write;

type ShareSession = Arc<Mutex<Session>>;

pub struct Host {
    nodeid: NodeId,
    keypair: KeyPair,
    sessions: Arc<RwLock<Slab<ShareSession>>>,               // 就绪状态,未就绪状态的会话
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
            sessions: Arc::new(RwLock::new(Slab::new())),
            version
        }
    }

    async fn process_new_connect(share_session: ShareSession, sessions: Arc<RwLock<Slab<ShareSession>>>) {
        let arc_session = share_session.clone();
        let key = sessions.write().insert(share_session);
        info!("create session, before session reading.");
        loop {
            let session_reading = arc_session.lock().await.reading().await;       // fixme: 错误处理
            for srr in session_reading {
                match srr {
                    SessionReadResult::Packet(data) => {
                        // fixme: 这个地方不会返回，应该在某个地方将消息发送到上层。
                        info!("host read {:?}", data);
                    },
                    SessionReadResult::Hub => {
                        info!("host read connection disconnected, remove.");
                        info!("sessions before remove key<{}>, {}", key, sessions.read().len());
                        sessions.write().remove(key);
                        info!("sessions after remove key<{}>, {}", key, sessions.read().len());
                    },
                    SessionReadResult::Error(e) => {
                        error!("host read error {}", e);
                        info!("sessions before remove key<{}>, {}", key, sessions.read().len());
                        sessions.write().remove(key);
                        info!("sessions after remove key<{}>, {}", key, sessions.read().len());
                    }
                }
            }
        }

        info!("process new connect done.");
    }

    /// 启动网络服务， 1. 开启监听；
    pub async fn start(&self, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(config.listen_addr.as_str()).await?;
        info!("listening on {}", config.listen_addr);
        loop {
            let (socket, addr) = listener.accept().await?;
            info!("accept tcp connection {}", addr);
            let sessions = self.sessions.clone();

            let mut session = Session::new(socket, None, self.version);
            let share_session = Arc::new(Mutex::new(session));

            tokio::spawn( async move {
                Host::process_new_connect(share_session, sessions).await;
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

        let sessions = self.sessions.clone();

        let session = Session::new(stream, Some(nodeid), self.version);
        let share_session = Arc::new(Mutex::new(session));
        let arc_session = share_session.clone();
        tokio::spawn( async move {
            Host::process_new_connect(share_session, sessions).await;
        });

        arc_session.lock().await.write_auth().await?;

        Ok(())
    }

    /// 定时任务，待完成
    pub async fn timer_task(&self) {
        info!("timer task, start ping-pong detect.");
        // fixme: KAD节点发现服务待实现。

        // 心跳检测
        let arc_sessions = self.sessions.clone();
        tokio::spawn( async move {
            Host::keep_alive(arc_sessions).await;
        });
        info!("start timer task done.");
    }

    async fn keep_alive(sessions: Arc<RwLock<Slab<ShareSession>>>) {
        loop {
            let mut remove_key = Vec::new();
            let arc_sessions = sessions.clone();
            info!("before sleep.");
            tokio::time::sleep(Duration::new(60, 0)).await;

            let mut pair = Vec::new();
            {
                for (k, s) in arc_sessions.write().iter() {
                    pair.push((k, s.clone()));
                }
            }

            for (k, s) in pair {
                info!("after sleep trace 1.");
                let mut lock = s.lock().await;
                info!("after sleep trace 2.");

                if let Err(e) = lock.keep_alive().await {
                    error!("session keep alive error: {}, remove {}.", e, k);
                    remove_key.push(k);
                }
                info!("session keep alive done.");
            }

            {
                let mut s = sessions.write();
                for key in remove_key {
                    s.remove(key);
                }
            }

        }
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