use tokio::net::TcpStream;
use crate::network::connection::{Connection, HEADER_LEN, ReadResult, Bytes};
use crate::crypto::NodeId;
use std::vec::Vec;
use crate::network::error::NetError;
use std::error::Error;

/**
 *  Header(16 bytes) [version(1 bytes) 协议版本号，区别与节点版本号，暂时不用| type_id(1 bytes) | cmd(2 bytes) | payload_len(4 bytes) | ...] | Payload (n bytes)
 *  Auth    1
 *  Ack     2
 *  Hello   3
 *  Ping    4
 *  Pong    5
 *  UserPacket  0xa
 */

pub const PROTOCOL_VERSION: u8 = 0;

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
}

impl Session {
    pub fn new(socket: TcpStream, remote: Option<NodeId>, version: u8) -> Self {
        Session {
            connection: Connection::new(socket),
            hello: false,
            remote,
            version
        }
    }

    pub fn set_remote(&mut self, nodeid: NodeId) {
        self.remote = Some(nodeid);
    }

    pub fn remote(&self) -> Option<&NodeId> {
        self.remote.as_ref()
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
        // fixme: ack的body暂时没有数据，payload_len=0
        if let Err(e) = self.connection.send(&packet).await {
            error!("write ack to {:?} failure: {}", self.remote, e);
            return Err(NetError::IoError(e.description().to_string()));
        }

        Ok(())
    }

    async fn read_ack(&mut self, data: &[u8]) -> Result<(), NetError> {
        info!("read_ack from {:?}, data: {:?}", self.remote, data);

        Ok(())
    }

    pub async fn reading(&mut self) -> SessionReadResult {
        loop {
            match self.connection.read().await {
                ReadResult::Packet(data) => {
                    // todo: 向上层返回
                    info!("session read: {:?}", data);
                    match data.id {
                        1u8 => {
                            if let Err(e) = self.read_auth(data.data.as_ref()).await {
                                return SessionReadResult::Error(e);
                            }
                        },
                        2u8 => {
                            if let Err(e) = self.read_ack(data.data.as_ref()).await {
                                return SessionReadResult::Error(e);
                            }
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
                    return SessionReadResult::Hub;
                },
                ReadResult::Error(e) => {
                    error!("session reading error: {}", e);
                    return SessionReadResult::Error(e);
                }
            }
        }
    }
}