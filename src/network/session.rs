use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use crate::network::connection::{Connection, HEADER_LEN, ReadResult, Bytes};
use crate::crypto::NodeId;
use std::vec::Vec;
use crate::network::error::NetError;
use std::error::Error;
use std::time::{Instant, Duration};
use std::cell::RefCell;

/**
 *  Header(16 bytes) [version(1 bytes) 协议版本号，区别与节点版本号，暂时不用| type_id(1 bytes) | cmd(2 bytes) | payload_len(4 bytes) | ...] | Payload (n bytes)
 *  Auth    1
 *  Ack     2
 *  Hello   3       // fixme: 暂不实现
 *  Ping    4
 *  Pong    5
 *  UserPacket  0xa
 */

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


pub enum SessionReadResult {
    Packet(Bytes),
    Hub,
    Error(NetError),
}

pub struct Session {
    connection: Connection,     // 底层数据传输
    hello: bool,                // 会话有没有成功建立，收到对方的hello包才算是完成会话建立，可以开始发送用户数据
    remote: Option<NodeId>,     // 远端节点ID,被动节点最开始时不知道对方的节点ID，主动方知道
    version: u8,
    ping: Instant,
}

impl Session {
    pub fn new(socket: TcpStream, remote: Option<NodeId>, version: u8) -> Self {
        Session {
            connection: Connection::new(socket),
            hello: false,
            remote,
            version,
            ping: Instant::now(),
        }
    }

    pub fn set_remote(&mut self, nodeid: NodeId) {
        self.remote = Some(nodeid);
    }

    pub fn remote(&self) -> Option<&NodeId> {
        self.remote.as_ref()
    }

    /// data: 用户上层数据
    pub async fn sending(&mut self, data: &[u8]) -> Result<(), NetError> {
        info!("send user data: {}", data.len());
        let payload_len = data.len();

        let mut header = vec![0u8, 0xau8, 0, 0];
        let mut len = vec![payload_len as u8, (payload_len >> 8) as u8, (payload_len >> 16) as u8, (payload_len >> 24) as u8];
        header.extend_from_slice(len.as_slice());
        header.extend_from_slice(&[0u8; 8]);

        let mut packet = Vec::new();
        packet.extend_from_slice(&header);
        packet.extend_from_slice(data);

        if let Err(e) = self.connection.send(&packet).await {
            error!("send user packet to {:?} failure: {}", self.connection.peer_addr(), e);
            return Err(NetError::IoError(e.description().to_string()));
        }

        Ok(())
    }

    // 主动发起方，向对方发送握手信息， 目前采用明文传输方式，直接将本节点ID传输至对方,实际上应该采用密码学的方式，recover出公钥，这里先忽略，待以后补充。
    pub async fn write_auth(&mut self) -> Result<(), Box<dyn std::error::Error>>{
        let remote = &self.remote;
        let remote = remote.unwrap();
        info!("write auth to remote {:?}", remote);
        let data = remote.as_bytes();
        debug!("raw data of H512 {:?}", data);
        let mut payload = Vec::new();
        payload.extend_from_slice(data);
        info!("raw payload {:?}", payload);
        let payload_len = payload.len() as u32;
        let mut header = vec![PROTOCOL_VERSION, PacketId::Auth as u8, 0, 0];
        let mut len = vec![payload_len as u8, (payload_len >> 8) as u8, (payload_len >> 16) as u8, (payload_len >> 24) as u8];
        info!("payload_len {} -> {:?}", payload_len, len);
        header.extend_from_slice(len.as_slice());
        header.extend_from_slice(&[0u8; 8]);
        info!("header: {:?}", header);

        let mut packet = Vec::new();
        packet.extend_from_slice(&header);
        packet.extend_from_slice(&payload);

        self.connection.send(&packet).await?;

        Ok(())
    }


    async fn read_auth(&mut self, data: &[u8]) -> Result<(), NetError> {
        info!("read auth from {:?}", self.connection.peer_addr());
        if data.len() != 64 {
            let reason = format!("read_auth data fault.");
            error!("{}", reason);
            return Err(NetError::DataFault(reason));
        }

        let nodeid = NodeId::from_slice(data);
        info!("read_auth, remote nodeid: {:?}", nodeid);

        self.remote = Some(nodeid);
        self.hello = true;      // fixme: 目前先忽略hello包，收到auth，ack就可以认为对方准备就绪了，hello包用来协商版本号的，因目前没有版本要求，暂时先忽略，待以后补充
        self.write_ack().await?;

        Ok(())
    }

    async fn write_ack(&mut self) -> Result<(), NetError> {
        info!("write ack to {:?}", self.remote);

        let mut header = vec![PROTOCOL_VERSION, PacketId::Ack as u8, 0, 0, 1];
        let res = vec![0u8; 11];
        header.extend_from_slice(res.as_slice());

        let payload = vec![0u8];

        let mut packet = Vec::new();
        packet.extend_from_slice(header.as_slice());
        packet.extend_from_slice(payload.as_slice());

        if let Err(e) = self.connection.send(&packet).await {
            error!("write ack to {:?} failure: {}", self.remote, e);
            return Err(NetError::IoError(e.description().to_string()));
        }

        Ok(())
    }

    async fn read_ack(&mut self, data: &[u8]) -> Result<(), NetError> {
        info!("read_ack from {:?}, data: {:?}", self.remote, data);
        self.hello = true;      // fixme: 目前先忽略hello包，收到auth，ack就可以认为对方准备就绪了，hello包用来协商版本号的，因目前没有版本要求，暂时先忽略，待以后补充

        Ok(())
    }

    pub async fn keep_alive(&mut self) -> Result<(), NetError> {
        // 还未就绪，不需要发送心跳包
        if self.hello == false {
            info!("session not receive hello, return.");
            return Ok(());
        }
        info!("keep alive, prepare to send ping to {:?}", self.connection.peer_addr());

        let now = Instant::now();
        if now.duration_since(self.ping) > Duration::new(PING_TIMEOUT, 0) {
            return Err(NetError::PingTimeout);
        }

        self.send_ping().await?;

        Ok(())
    }

    async fn send_ping(&mut self) -> Result<(), NetError> {
        info!("send ping to {:?}", self.connection.peer_addr());
        let mut header = vec![PROTOCOL_VERSION, PacketId::Ping as u8, 0, 0, 1];
        let res = vec![0u8; 11];
        header.extend_from_slice(res.as_slice());

        let payload = vec![0u8];

        let mut packet =  Vec::new();
        packet.extend_from_slice(header.as_slice());
        packet.extend_from_slice(payload.as_slice());

        if let Err(e) = self.connection.send(&packet).await {
            error!("send ping to {:?} failure: {}", self.connection.peer_addr(), e);
            return Err(NetError::IoError(e.description().to_string()));
        }

        self.ping = Instant::now();

        Ok(())
    }

    async fn read_ping(&mut self) -> Result<(), NetError> {
        info!("read ping from {:?}", self.connection.peer_addr());
        self.send_pong().await?;

        Ok(())
    }

    async fn send_pong(&mut self) -> Result<(), NetError> {
        info!("send pong to {:?}", self.connection.peer_addr());
        let mut header = vec![PROTOCOL_VERSION, PacketId::Pong as u8, 0, 0, 1];
        let res = vec![0u8; 11];
        header.extend_from_slice(res.as_slice());

        let payload = vec![0u8];

        let mut packet =  Vec::new();
        packet.extend_from_slice(header.as_slice());
        packet.extend_from_slice(payload.as_slice());

        if let Err(e) = self.connection.send(&packet).await {
            error!("send pong to {:?} failure: {}", self.connection.peer_addr(), e);
            return Err(NetError::IoError(e.description().to_string()));
        }

        Ok(())
    }

    async fn read_pong(&mut self) -> Result<(), NetError> {
        info!("read pong from {:?}", self.connection.peer_addr());
        let now = Instant::now();
        let span = now.duration_since(self.ping);
        if span > Duration::new(PING_TIMEOUT, 0) {
            warn!("ping timeout {:?}", self.connection.peer_addr());
            return Err(NetError::PingTimeout);
        }
        Ok(())
    }



    pub async fn reading(&mut self) -> Vec<SessionReadResult> {
        let r = self.connection.read().await;
        let mut srr = Vec::new();

        for read_result in r {
            match read_result {
                ReadResult::Packet(data) => {
                    // todo: 向上层返回
                    info!("session read: {:?}", data);
                    match data.id {
                        1u8 => {
                            if let Err(e) = self.read_auth(data.data.as_ref()).await {
                                // 出现认证错误，立即返回
                                return vec![SessionReadResult::Error(e)];
                            }
                        },
                        2u8 => {
                            if let Err(e) = self.read_ack(data.data.as_ref()).await {
                                // 出现认证错误，立即返回
                                return vec![SessionReadResult::Error(e)];
                            }
                        },
                        4u8 => {
                            if let Err(e) = self.read_ping().await {
                                // ping-pong检测失败，前面收到的包依旧需要处理
                                srr.push(SessionReadResult::Error(e));
                            }
                        },
                        5u8 => {
                            if let Err(e) = self.read_pong().await {
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

                    // continue;
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
}