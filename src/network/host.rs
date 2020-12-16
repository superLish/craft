use tokio::net::{TcpListener, TcpStream};
use std::str::FromStr;
use crate::network::{NodeId, NetPacket};
use crate::config::Config;
use parity_crypto::publickey::{Generator, KeyPair, Public, Random, recover, Secret, sign, ecdh, ecies};
use ethereum_types::H512;
use std::sync::Arc;
use tokio::sync::RwLock;
use slab::Slab;
use tokio::sync::mpsc;
use super::session::{SessionEvent, ShareSession, SessionWrite, SessionRead, SessionState, SessionReadResult, channel_recv};
use super::NetEvent;


pub struct Host {
    nodeid: NodeId,
    keypair: KeyPair,
    sessions: Arc<RwLock<Slab<ShareSession>>>,               // 就绪状态,未就绪状态的会话
    net_sender: mpsc::Sender<NetEvent>,
}

impl Host {
    pub fn new(config: &Config, net_sender: mpsc::Sender<NetEvent>) -> Self {
        let secret = Secret::copy_from_str(config.secret.as_str()).unwrap();
        let keypair = KeyPair::from_secret(secret).unwrap();
        let nodeid = keypair.public().clone();
        debug!("keypair: {}, nodeid: {:?}", keypair, nodeid);
        
        Host {
            nodeid,
            keypair,
            sessions: Arc::new(RwLock::new(Slab::new())),
            net_sender,
        }
    }

    async fn process_new_connect(stream: TcpStream, local: NodeId, mut remote: Option<NodeId>, sessions: Arc<RwLock<Slab<ShareSession>>>, net_sender: mpsc::Sender<NetEvent>) {
        let is_active = remote.is_some();
        let (tx_session, mut rx_session) = mpsc::channel(1024);
        let share_session = ShareSession::new(remote.clone(), tx_session.clone());
        let key = sessions.write().await.insert(share_session);
        debug!("session new key: {}", key);

        let state = SessionState::new(remote.clone());
        let state = Arc::new(RwLock::new(state));

        let (r, w) = TcpStream::into_split(stream);
        let mut sr = SessionRead::new(r, tx_session.clone());
        let mut sw = SessionWrite::new(w, tx_session.clone());

        if is_active {
            tx_session.send(SessionEvent::SendAuth(local)).await.unwrap();
        }

        'outer: loop {
            let result = tokio::select! {
                v1 = sr.reading(state.clone()) => {
                    debug!("reading task complete. {:?}", v1);
                    for srr in v1 {
                         match srr {
                            SessionReadResult::NewConnection => {
                                info!("host, new connection, update state.");
                                if let Some(s) = sessions.write().await.get_mut(key) {
                                    if s.nodeid.is_none() {
                                        let node = state.read().await.remote.clone();
                                        s.nodeid = node.clone();
                                        remote = node;
                                    }
                                    net_sender.send(NetEvent::Connected(s.nodeid.clone().unwrap())).await.unwrap();
                                } else {
                                    // fixme: 错误处理
                                    error!("sessions get key {} error. {:?}", key, state.read().await.remote);
                                }
                            },
                            SessionReadResult::Packet(ref data) => {
                                let packet = NetPacket::new(remote.unwrap(), data);
                                info!("host read {:?}", packet);
                                net_sender.send(NetEvent::Read(packet)).await.unwrap();
                            },
                            SessionReadResult::Hub => {
                                info!("host read connection disconnected, remove.");
                                // info!("sessions before remove key<{}>, {}", key, sessions.read().await.len());
                                sessions.write().await.remove(key);
                                net_sender.send(NetEvent::Disconnected(remote.unwrap())).await.unwrap();
                                // info!("sessions after remove key<{}>, {}", key, sessions.read().await.len());
                                break 'outer;
                            },
                            SessionReadResult::Error(e) => {
                                error!("host read error {}", e);
                                sessions.write().await.remove(key);
                                net_sender.send(NetEvent::Disconnected(remote.unwrap())).await.unwrap();
                                break 'outer;
                            }
                        }
                    }
                }
                v2 = channel_recv(&mut rx_session, &mut sw, state.clone()) => {
                    debug!("channel recv task complete.");
                    if let Err(e) = v2 {
                        // fixme: 发送错误，目前的处理是删除掉该连接， 后面可根据具体的错误，看是否有自修复的可能
                        error!("host read session channel recv error: {}", e);
                        sessions.write().await.remove(key);
                        net_sender.send(NetEvent::Disconnected(remote.unwrap())).await.unwrap();
                        break 'outer;
                    }
                }
            };
        }

        info!("process session {:?} done.", remote);
    }

    /// 启动网络服务，
    pub async fn start(&self, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
        // 1. 开启监听；
        let listener = TcpListener::bind(config.listen_addr.as_str()).await?;
        info!("listening on {}", config.listen_addr);
        let sessions = self.sessions.clone();
        let sender = self.net_sender.clone();
        let local_nodeid = self.nodeid.clone();
        tokio::spawn( async move {
            loop {
                let net_sender = sender.clone();
                let local = local_nodeid.clone();
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        info!("accept new tcp connection {}", addr);
                        let arc_sessions = sessions.clone();
                        tokio::spawn( async move {
                            Host::process_new_connect(stream, local, None, arc_sessions, net_sender).await;
                        });
                    },
                    Err(e) => {
                        error!("listener error: {}", e);
                    }
                }
            }
        });

        // 2. 开始定时任务
        self.timer_task().await;

        Ok(())
    }

    /// 主动连接到种子节点
    pub async fn connect_seed(&self, seed: &str) -> Result<(), Box<dyn std::error::Error>> {
        info!("prepare to connect to seed {}", seed);
        let (nodeid, addr) = enode_str_parse(seed)?;
        let stream = TcpStream::connect(addr).await?;
        info!("connected to {}, prepare to estabilsh session.", seed);

        let sessions = self.sessions.clone();
        let sender = self.net_sender.clone();
        let local_nodeid = self.nodeid.clone();
        tokio::spawn( async move {
            Host::process_new_connect(stream, local_nodeid, Some(nodeid), sessions, sender.clone()).await;
        });

        Ok(())
    }

    pub async fn send_packet(&self, data: &NetPacket) -> Result<(), Box<dyn std::error::Error>> {
        for (_k, s) in self.sessions.read().await.iter() {
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
    async fn timer_task(&self) {
        info!("timer task, start ping-pong detect.");
        // fixme: KAD节点发现服务待实现。

        // 心跳检测
        let sessions = self.sessions.clone();
        tokio::spawn( async move {
            Host::keep_alive(sessions).await;
        });
        info!("start timer task done.");
    }

    async fn keep_alive(sessions: Arc<RwLock<Slab<ShareSession>>>) {
        loop {
            let arc_sessions = sessions.clone();
            tokio::time::sleep(tokio::time::Duration::new(60, 0)).await;
            for (k, s) in arc_sessions.read().await.iter() {
                if let Err(e) = s.sender.send(SessionEvent::KeepAlive).await {
                    error!("channel send ping event error: {}", e);
                }
            }
        }
    }
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