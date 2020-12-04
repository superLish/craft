use tokio::net::TcpStream;
use crate::network::connection::{Connection, HEADER_LEN};
use crate::crypto::NodeId;
use std::vec::Vec;

/**
 *  Header(16 bytes) | Payload
 *  
 */


pub const MAX_PACKET_SIZE: usize = 16 * 1024 * 1024;   // 限制应用数据包的大小为16Mb


pub struct Session {
    connection: Connection,     // 底层数据传输
    hello: bool,                // 会话有没有成功建立，收到对方的hello包才算是完成会话建立，可以开始发送用户数据
    remote: Option<NodeId>,     // 远端节点ID,被动节点最开始时不知道对方的节点ID，主动方知道
}

impl Session {
    pub fn new(socket: TcpStream, remote: Option<NodeId>) -> Self {
        Session {
            connection: Connection::new(socket),
            hello: false,
            remote,
        }
    }

    pub fn set_remote(&mut self, nodeid: NodeId) {
        self.remote = Some(nodeid);
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
        let mut header = vec![payload_len as u8, (payload_len >> 8) as u8, (payload_len >> 16) as u8, (payload_len >> 24) as u8];
        info!("payload_len {} -> {:?}", payload_len, header);
        header.extend_from_slice(&[0u8; 12]);
        info!("header: {:?}", header);

        let mut packet = Vec::new();
        packet.extend_from_slice(&header);
        packet.extend_from_slice(&payload);

        self.connection.send(&packet).await?;

        Ok(())
    }
}