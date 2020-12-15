use tokio::net::TcpStream;
use tokio::prelude::*;
use tokio::io::AsyncRead;
use crate::network::session::PROTOCOL_VERSION;
use crate::network::error::NetError;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

pub const HEADER_LEN: usize = 16;


pub enum ReadResult {
    Packet(Packet),       // 读到的payload数据， header数据不向上层返回
    Hub,                  // 读到0，节点断开
    Error(NetError),      // 读出错
}

#[derive(Debug)]
pub struct Packet {
    pub id: u8,
    pub data: Bytes,
}

#[derive(Debug, Clone)]
pub enum Stage {
    Header,
    Body(u8),       // packet_id
}

pub type Bytes = Vec<u8>;

pub struct ConnectionWrite {
    socket: OwnedWriteHalf,
}

impl ConnectionWrite {
    pub fn new(socket: OwnedWriteHalf) -> Self {
        ConnectionWrite {
            socket
        }
    }

    pub async fn send(&mut self, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        info!("prepare to write {} bytes to remote: {:?}", data.len(), self.socket.as_ref().peer_addr());
        self.socket.write_all(data).await?;
        info!("write {} bytes to remote: {:?} done.", data.len(), self.socket.as_ref().peer_addr());

        Ok(())
    }

    pub fn peer_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.as_ref().peer_addr()
    }
}


// 负责TCP层次的数据处理
pub struct ConnectionRead {
    socket: OwnedReadHalf,          // 底层TCP传输
    expect: usize,              // TCP流传输，需要自己切分数据流
    buffer: Bytes,              // 接收数据缓存
    stage: Stage,
}

impl ConnectionRead {
    pub fn new(socket: OwnedReadHalf) -> Self {
        ConnectionRead {
            socket,
            expect: HEADER_LEN,
            buffer: Bytes::new(),
            stage: Stage::Header,       // 初始状态为Header
        }
    }

    pub fn expect(&mut self, expect: usize, stage: Stage) {
        self.expect = expect;
        self.stage = stage;
    }

    pub async fn read(&mut self) -> Vec<ReadResult> {
        let mut rawbuf = [0u8; 32];

        // 这个loop循环是从Socket读字节流，一次完整的读包操作
        loop {
            let n = match self.socket.read(&mut rawbuf).await {
                Ok(n) if 0 == n => {
                    info!("read 0, {:?} connection end.", self.socket.as_ref().peer_addr());
                    // todo: 连接断开， 从sessions中删除会话
                    return vec![ReadResult::Hub];
                },
                Ok(n) => {
                    info!("read {} bytes in rawbuf: {:?}", n, rawbuf);
                    self.buffer.extend_from_slice(&rawbuf[0..n]);

                    let mut result = Vec::new();
                    while self.buffer.len() >= self.expect {
                        if let Some(r) = self.check_buffer().await {
                            result.push(r);
                        }
                    }
                    return result;
                },
                Err(e) => {
                    let reason = format!("{}", e);
                    error!("read failure from {:?}, {}", self.socket.as_ref().peer_addr(), reason);
                    return vec![ReadResult::Error(NetError::IoError(reason))];
                },
            };
        }
    }

    async fn check_buffer(&mut self) -> Option<ReadResult> {
        assert!(self.buffer.len() >= self.expect);
        // 已经接收到一个完整的包头或者包体
        match self.stage {
            Stage::Header => {
                if let Err(e) = self.read_header() {
                    return Some(ReadResult::Error(e));
                }
            },
            Stage::Body(id) => {
                let body = self.read_body();
                return Some(ReadResult::Packet(Packet {id, data: body}));
            }
        }

        None
    }

    fn read_header(&mut self) -> Result<(), NetError> {
        info!("read_header: buffer({}) {:?}", self.buffer.len(), self.buffer);
        let header = &self.buffer[0..HEADER_LEN];
        info!("read_header: header {:?}", header);

        // check protocol version.
        let version = header[0];
        if version != PROTOCOL_VERSION {
            let reason = format!("mismatch protocol version. local version: {}, remote version: {}", PROTOCOL_VERSION, version);
            error!("{}", reason);
            return Err(NetError::ProtocolMismatch(reason));
        }

        // type id
        let packet_id = header[1];

        // todo: command目前先忽略

        // let payload_len = (header[0] as u32) + (header[1] as u32)<<8 + (header[2] as u32)<<16 + (header[3] as u32)<<24;
        let h0 = header[4] as u32;
        let h1 = (header[5] as u32) << 8;
        let h2 = (header[6] as u32) << 16;
        let h3 = (header[7] as u32) << 24;
        debug!("header: {} {} {} {}", h0, h1, h2, h3);
        let payload_len = h0 + h1 + h2 + h3;
        info!("read header: payload_len={}", payload_len);
        self.expect(payload_len as usize, Stage::Body(packet_id));
        let buffer_len = self.buffer.len();
        let res = self.buffer[HEADER_LEN..buffer_len].to_vec();
        info!("read_header: res {:?}", res);
        self.buffer.clear();
        self.buffer.extend_from_slice(res.as_slice());
        info!("read_header, after copy: buffer {:?}", self.buffer);

        Ok(())
    }

    fn read_body(&mut self) -> Bytes {
        info!("read_body: buffer {:?}", self.buffer);
        let buffer_len = self.buffer.len();
        let mut packet = Vec::new();
        packet.extend_from_slice(&self.buffer[0..self.expect]);
        let res = self.buffer[self.expect..buffer_len].to_vec();
        self.buffer.clear();
        self.buffer.extend_from_slice(res.as_slice());
        info!("read_body, after copy: buffer {:?}", self.buffer);
        self.expect(HEADER_LEN, Stage::Header);

        packet
    }

    pub fn peer_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.as_ref().peer_addr()
    }
}