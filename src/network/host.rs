use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use std::collections::{HashMap, HashSet};
use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::str::FromStr;
use crate::network::{NodeId, NetPacket};
use crate::config::Config;
use parity_crypto::publickey::{Generator, KeyPair, Public, Random, recover, Secret, sign, ecdh, ecies};
use ethereum_types::H512;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{Interval, Duration};
use slab::Slab;
use std::io::Write;
use tokio::sync::mpsc;
use super::connection::Bytes;
use super::session::{SessionEvent, ShareSession, SessionWrite, SessionRead, SessionState, SessionReadResult, channel_recv};
use crate::network::connection::ConnectionRead;


pub struct Host {
    nodeid: NodeId,
    keypair: KeyPair,
    sessions: Arc<RwLock<Slab<ShareSession>>>,               // 就绪状态,未就绪状态的会话
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
            sessions: Arc::new(RwLock::new(Slab::new())),
        }
    }

    async fn process_new_connect(stream: TcpStream, remote: Option<NodeId>, sessions: Arc<RwLock<Slab<ShareSession>>>) {
        let is_active = remote.is_some();
        let (tx_session, mut rx_session) = mpsc::channel(1024);
        let share_session = ShareSession::new(remote.clone(), tx_session.clone());
        let key = sessions.write().await.insert(share_session);
        info!("session new key: {}", key);

        let state = SessionState::new(remote);
        let state = Arc::new(RwLock::new(state));

        let (r, w) = TcpStream::into_split(stream);
        let mut sr = SessionRead::new(r, tx_session.clone());
        let mut sw = SessionWrite::new(w, tx_session.clone());

        if is_active {
            tx_session.send(SessionEvent::SendAuth).await.unwrap();
        }

        'outer: loop {
            let result = tokio::select! {
                v1 = sr.reading(state.clone()) => {
                    info!("reading task complete. {:?}", v1);
                    for srr in v1 {
                         match srr {
                            SessionReadResult::NewConnection => {
                                info!("host, new connection, update state.");
                                if let Some(s) = sessions.write().await.get_mut(key) {
                                    if s.nodeid.is_none() {
                                        s.nodeid = state.read().await.remote.clone();
                                    }
                                }
                            },
                            SessionReadResult::Packet(data) => {
                                // fixme: 这个地方不会返回，应该在某个地方将消息发送到上层。
                                info!("host read {:?}", data);
                            },
                            SessionReadResult::Hub => {
                                info!("host read connection disconnected, remove.");
                                info!("sessions before remove key<{}>, {}", key, sessions.read().await.len());
                                sessions.write().await.remove(key);
                                info!("sessions after remove key<{}>, {}", key, sessions.read().await.len());
                                break 'outer;
                            },
                            SessionReadResult::Error(e) => {
                                error!("host read error {}", e);
                                sessions.write().await.remove(key);
                                break 'outer;
                            }
                        }
                    }
                }
                v2 = channel_recv(&mut rx_session, &mut sw, state.clone()) => {
                    info!("channel recv task complete.");
                }
            };
        }

        info!("process new connect {:?} done.", state.read().await.remote);
    }

    /// 启动网络服务， 1. 开启监听；
    pub async fn start(&self, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(config.listen_addr.as_str()).await?;
        info!("listening on {}", config.listen_addr);
        let sessions = self.sessions.clone();
        tokio::spawn( async move {
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        info!("accept new tcp connection {}", addr);
                        let arc_sessions = sessions.clone();
                        tokio::spawn( async move {
                            Host::process_new_connect(stream, None, arc_sessions).await;
                        });
                    },
                    Err(e) => {
                        error!("listener error: {}", e);
                    }
                }
            }
        });

        Ok(())
    }

    /// 主动连接到种子节点
    pub async fn connect_seed(&self, seed: &str) -> Result<(), Box<dyn std::error::Error>> {
        info!("prepare to connect to seed {}", seed);
        let (nodeid, addr) = enode_str_parse(seed)?;
        let stream = TcpStream::connect(addr).await?;
        info!("connected to {}, prepare to estabilsh session.", seed);

        let sessions = self.sessions.clone();
        tokio::spawn( async move {
            Host::process_new_connect(stream, Some(nodeid), sessions).await;
        });

        Ok(())
    }

    pub async fn send_packet(&self, data: &NetPacket) -> Result<(), Box<dyn std::error::Error>> {
        for (k, s) in self.sessions.read().await.iter() {
            if s.nodeid.is_none() {
                continue;
            }
            if s.nodeid.unwrap() == data.nodeid {
                s.sender.send(SessionEvent::Send(data.data.clone())).await?;
                return Ok(());
            }
        }

        warn!("send packet to remote failure, reason: remote is not ready. {:?}", data.nodeid);
        Ok(())
    }

    /// 定时任务，待完成
    pub async fn timer_task(&self) {
        info!("timer task, start ping-pong detect.");
        // fixme: KAD节点发现服务待实现。

        // 心跳检测
        // let arc_sessions = self.sessions.clone();
        // tokio::spawn( async move {
        //     Host::keep_alive(arc_sessions).await;
        // });
        info!("start timer task done.");
    }
/*
    async fn keep_alive(sessions: Arc<RwLock<Slab<Session>>>) {
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

            for (k, mut s) in pair {
                // info!("after sleep trace 1.");
                // let mut lock = s.lock().await;
                // info!("after sleep trace 2.");

                if let Err(e) = s.keep_alive().await {
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
*/
}

pub fn enode_str_parse(node: &str) -> Result<(NodeId, &str), Box<dyn std::error::Error>> {
    info!("node is {}", node);
    let pubkey = &node[8..136];
    info!("pubkey is {}", pubkey);
    let nodeid = H512::from_str(pubkey)?;
    let addr = &node[137..];
    info!("socket addr is {}", addr);
    
    Ok((nodeid, addr))
}