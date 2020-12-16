use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use crate::network::connection::{HEADER_LEN, ReadResult, Bytes, ConnectionRead, ConnectionWrite};
use crate::crypto::NodeId;
use std::vec::Vec;
use crate::network::error::NetError;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use std::sync::Arc;

/**
 *  Header(16 bytes) [version(1 bytes) 协议版本号，区别与节点版本号，暂时不用| type_id(1 bytes) | cmd(2 bytes) | payload_len(4 bytes) | ...] | Payload (n bytes)
 *  Auth    1
 *  Ack     2
 *  Hello   3       // fixme: 暂不实现
 *  Ping    4
 *  Pong    5
 *  UserPacket  0xa
 */

pub struct ShareSession {
    pub nodeid: Option<NodeId>,
    pub sender: tokio::sync::mpsc::Sender<SessionEvent>
}

impl ShareSession {
    pub fn new(nodeid: Option<NodeId>, sender: mpsc::Sender<SessionEvent>) -> Self {
        ShareSession {
            nodeid,
            sender,
        }
    }
}

#[derive(Debug)]
pub enum SessionEvent {
    SendAuth(NodeId),   // 发起握手认证 SendAuth(local nodeid)
    SendAck,
    Send(Bytes),        // 发送数据
    SendPing,
    SendPong,
    KeepAlive,
}

pub const PROTOCOL_VERSION: u8 = 0;
const PING_TIMEOUT: u64 = 5;         // ping超时设置为5s

pub enum PacketId {
    Auth = 1,
    Ack = 2,
    Hello = 3,
    Ping = 4,
    Pong = 5,
    UserPacket = 0xa,
}


pub const MAX_PACKET_SIZE: usize = 16 * 1024 * 1024;   // 限制应用数据包的大小为16Mb

#[derive(Debug)]
pub enum SessionReadResult {
    Packet(Bytes),
    Hub,
    NewConnection,      // 这里的新连接指的是已经完成应用协议握手的连接，而非Tcp连接
    Error(NetError),
}

pub struct SessionState {
    pub hello: bool,                // 会话有没有成功建立，收到对方的hello包才算是完成会话建立，可以开始发送用户数据
    pub remote: Option<NodeId>,     // 远端节点ID,被动节点最开始时不知道对方的节点ID，主动方知道
    pub ping: Option<Instant>,
}

impl SessionState {
    pub fn new(remote: Option<NodeId>) -> Self {
        SessionState {
            hello: false,
            remote,
            ping: None,
        }
    }
}

pub async fn channel_recv(receiver: &mut mpsc::Receiver<SessionEvent>, sw: &mut SessionWrite, state: Arc<RwLock<SessionState>>) -> Result<(), NetError> {
    if let Some(event) = receiver.recv().await {
        match event {
            SessionEvent::SendAuth(ref local) => {
                sw.write_auth(state, local).await?;
            },
            SessionEvent::SendAck => {
                sw.write_ack(state).await?;
            },
            SessionEvent::Send(data) => {
                info!("send packet to remote.");
                sw.sending(&data, state).await?;
            },
            SessionEvent::SendPing => {
                sw.send_ping(state).await?;
            },
            SessionEvent::SendPong => {
                sw.send_pong().await?;
            },
            SessionEvent::KeepAlive => {
                sw.keep_alive(state).await?;
            },
        }
    } else {
        // fixme: 错误处理
        warn!("session event channel recv none. {:?}", state.read().await.remote);
    }

    Ok(())
}

pub struct SessionWrite {
    write: ConnectionWrite,
    sender: mpsc::Sender<SessionEvent>,
}

impl SessionWrite {
    pub fn new(write: OwnedWriteHalf, sender: mpsc::Sender<SessionEvent>) -> Self {
        SessionWrite {
            write: ConnectionWrite::new(write),
            sender,
        }
    }

    /// data: 用户上层数据
    pub async fn sending(&mut self, data: &[u8], state: Arc<RwLock<SessionState>>) -> Result<(), NetError> {
        let payload_len = data.len();
        info!("send user data({}) to remote: {:?}", payload_len, state.read().await.remote);

        let mut header = vec![0u8, 0xau8, 0, 0];
        let len = vec![payload_len as u8, (payload_len >> 8) as u8, (payload_len >> 16) as u8, (payload_len >> 24) as u8];
        header.extend_from_slice(len.as_slice());
        header.extend_from_slice(&[0u8; 8]);

        let mut packet = Vec::new();
        packet.extend_from_slice(&header);
        packet.extend_from_slice(data);

        if let Err(e) = self.write.send(&packet).await {
            let reason = format!("{}", e);
            error!("send user packet to {:?} failure: {}", self.write.peer_addr(), reason);
            return Err(NetError::IoError(reason));
        }

        Ok(())
    }

    // fixme: 主动发起方，向对方发送握手信息， 目前采用明文传输方式，直接将本节点ID传输至对方,实际上应该采用密码学的方式，recover出公钥，这里先忽略，待以后补充。
    pub async fn write_auth(&mut self, state: Arc<RwLock<SessionState>>, local: &NodeId) -> Result<(), NetError>{
        let remote = state.read().await.remote.clone();
        let remote = remote.unwrap();
        info!("write auth to remote {:?}", remote);
        let data = local.as_bytes();
        let mut payload = Vec::new();
        payload.extend_from_slice(data);
        debug!("raw payload {:?}", payload);
        let payload_len = payload.len() as u32;
        let mut header = vec![PROTOCOL_VERSION, PacketId::Auth as u8, 0, 0];
        let len = vec![payload_len as u8, (payload_len >> 8) as u8, (payload_len >> 16) as u8, (payload_len >> 24) as u8];
        info!("payload_len {} -> {:?}", payload_len, len);
        header.extend_from_slice(len.as_slice());
        header.extend_from_slice(&[0u8; 8]);
        info!("header: {:?}", header);

        let mut packet = Vec::new();
        packet.extend_from_slice(&header);
        packet.extend_from_slice(&payload);

        if let Err(e) = self.write.send(&packet).await {
            let reason = format!("{}", e);
            error!("write auth to {:?} failure: {}", remote, e);
            return Err(NetError::IoError(reason));
        }

        Ok(())
    }

    pub async fn write_ack(&mut self, state: Arc<RwLock<SessionState>>) -> Result<(), NetError> {
        let remote = state.read().await.remote.clone();
        info!("write ack to {:?}", remote);

        let mut header = vec![PROTOCOL_VERSION, PacketId::Ack as u8, 0, 0, 1];
        let res = vec![0u8; 11];
        header.extend_from_slice(res.as_slice());

        let payload = vec![0u8];
        let mut packet = Vec::new();
        packet.extend_from_slice(header.as_slice());
        packet.extend_from_slice(payload.as_slice());

        if let Err(e) = self.write.send(&packet).await {
            let reason = format!("{}", e);
            error!("write ack to {:?} failure: {}", remote, e);
            return Err(NetError::IoError(reason));
        }

        Ok(())
    }

    async fn keep_alive(&mut self, state: Arc<RwLock<SessionState>>) -> Result<(), NetError> {
        // 还未就绪，不需要发送心跳包
        if state.read().await.hello == false {
            info!("session not receive hello, return.");
            return Ok(());
        }
        info!("keep alive, prepare to send ping to {:?}", self.write.peer_addr());

        let now = Instant::now();
        if state.read().await.ping.is_some() {
            if now.duration_since(state.read().await.ping.clone().unwrap()) > std::time::Duration::new(PING_TIMEOUT, 0) {
                return Err(NetError::PingTimeout);
            }
        }

        self.send_ping(state).await?;

        Ok(())
    }

    async fn send_ping(&mut self, state: Arc<RwLock<SessionState>>) -> Result<(), NetError> {
        info!("send ping to {:?}", self.write.peer_addr());
        let mut header = vec![PROTOCOL_VERSION, PacketId::Ping as u8, 0, 0, 1];
        let res = vec![0u8; 11];
        header.extend_from_slice(res.as_slice());

        let payload = vec![0u8];

        let mut packet =  Vec::new();
        packet.extend_from_slice(header.as_slice());
        packet.extend_from_slice(payload.as_slice());

        if let Err(e) = self.write.send(&packet).await {
            error!("send ping to {:?} failure: {}", self.write.peer_addr(), e);
            return Err(NetError::IoError(e.description().to_string()));
        }

        state.write().await.ping = Some(Instant::now());

        Ok(())
    }

    async fn send_pong(&mut self) -> Result<(), NetError> {
        info!("send pong to {:?}", self.write.peer_addr());
        let mut header = vec![PROTOCOL_VERSION, PacketId::Pong as u8, 0, 0, 1];
        let res = vec![0u8; 11];
        header.extend_from_slice(res.as_slice());

        let payload = vec![0u8];

        let mut packet =  Vec::new();
        packet.extend_from_slice(header.as_slice());
        packet.extend_from_slice(payload.as_slice());

        if let Err(e) = self.write.send(&packet).await {
            error!("send pong to {:?} failure: {}", self.write.peer_addr(), e);
            return Err(NetError::IoError(e.description().to_string()));
        }

        Ok(())
    }
}


pub struct SessionRead {
    read: ConnectionRead,
    sender: mpsc::Sender<SessionEvent>,
}

impl SessionRead {
    pub fn new(read: OwnedReadHalf, sender: mpsc::Sender<SessionEvent>) -> Self {
        SessionRead {
            read: ConnectionRead::new(read),
            sender,
        }
    }

    pub async fn reading(&mut self, state: Arc<RwLock<SessionState>>) -> Vec<SessionReadResult> {
        let r = self.read.read().await;
        let mut srr = Vec::new();

        for read_result in r {
            match read_result {
                ReadResult::Packet(data) => {
                    // todo: 向上层返回
                    info!("session read: {:?}", data);
                    match data.id {
                        1u8 => {
                            if let Err(e) = self.read_auth(data.data.as_ref(), state.clone()).await {
                                return vec![SessionReadResult::Error(e)];       // 出现认证错误，放弃后面的数据，立即返回
                            } else {
                                srr.push(SessionReadResult::NewConnection);     // fixme: 暂时先认为收到auth或者ack即完成握手连接
                            }
                        },
                        2u8 => {
                            if let Err(e) = self.read_ack(data.data.as_ref(), state.clone()).await {
                                return vec![SessionReadResult::Error(e)];       // 出现认证错误，放弃后面的数据，立即返回
                            } else {
                                srr.push(SessionReadResult::NewConnection);
                            }
                        },
                        4u8 => {
                            if let Err(e) = self.read_ping().await {
                                // ping-pong检测失败，前面收到的包依旧需要处理
                                srr.push(SessionReadResult::Error(e));
                            }
                        },
                        5u8 => {
                            if let Err(e) = self.read_pong(state.clone()).await {
                                srr.push(SessionReadResult::Error(e));
                            }
                        },
                        0xau8 => {
                            // 用户数据，向上层返回
                            srr.push(SessionReadResult::Packet(data.data));
                        },
                        _ => {
                            panic!("unknow packet.");
                        }
                    }
                },
                ReadResult::Hub => {
                    // todo: 断开连接
                    info!("session connection disconnected.");
                    srr.push(SessionReadResult::Hub);
                },
                ReadResult::Error(e) => {
                    error!("session reading error: {}", e);
                    srr.push(SessionReadResult::Error(e));
                }
            }
        }

        srr
    }

    async fn channel_send(&self, data: SessionEvent) -> Result<(), NetError> {
        if let Err(e) = self.sender.send(data).await {
            let reason = format!("{}", e);
            error!("session channel send error: {}", reason);
            return Err(NetError::ChannelSendError(reason));
        }

        Ok(())
    }

    async fn read_auth(&self, data: &[u8], state: Arc<RwLock<SessionState>>) -> Result<(), NetError> {
        info!("read auth from {:?}", self.read.peer_addr());
        if data.len() != 64 {
            let reason = format!("read_auth data fault.");
            error!("{}", reason);
            return Err(NetError::DataFault(reason));
        }

        let nodeid = NodeId::from_slice(data);
        info!("read_auth, remote nodeid: {:?}", nodeid);

        state.write().await.remote = Some(nodeid);
        state.write().await.hello = true;           // fixme: 目前先忽略hello包，收到auth，ack就可以认为对方准备就绪了，hello包用来协商版本号的，因目前没有版本要求，暂时先忽略，待以后补充
        self.channel_send(SessionEvent::SendAck).await?;

        Ok(())
    }

    async fn read_ack(&self, data: &[u8], state: Arc<RwLock<SessionState>>) -> Result<(), NetError> {
        info!("read_ack from {:?}, data: {:?}", state.read().await.remote, data);
        state.write().await.hello = true;      // fixme: 目前先忽略hello包，收到auth，ack就可以认为对方准备就绪了，hello包用来协商版本号的，因目前没有版本要求，暂时先忽略，待以后补充

        Ok(())
    }

    async fn read_ping(&mut self) -> Result<(), NetError> {
        info!("read ping from {:?}", self.read.peer_addr());
        self.channel_send(SessionEvent::SendPong).await?;

        Ok(())
    }

    async fn read_pong(&mut self, state: Arc<RwLock<SessionState>>) -> Result<(), NetError> {
        info!("read pong from {:?}", self.read.peer_addr());
        let now = Instant::now();
        let span = now.duration_since(state.read().await.ping.clone().unwrap());
        if span > std::time::Duration::new(PING_TIMEOUT, 0) {
            warn!("ping timeout {:?}", self.read.peer_addr());
            return Err(NetError::PingTimeout);
        }

        state.write().await.ping = None;

        Ok(())
    }
}