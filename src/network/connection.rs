use tokio::net::TcpStream;
use tokio::prelude::*;
use tokio::io::AsyncRead;

pub const HEADER_LEN: usize = 16;

pub enum ReadResult {
    Packet(Bytes),      // 读到的payload数据， header数据不向上层返回
    Hub,                // 读到0，节点断开

}

pub type Bytes = Vec<u8>;


// 负责TCP层次的数据处理
pub struct Connection {
    socket: TcpStream,          // 底层TCP传输
    expect: usize,              // TCP流传输，需要自己切分数据流
    header_buffer: Bytes,       // header接收数据缓存
    payload_buffer: Bytes,      // payload接收数据缓存
}

impl Connection {
    pub fn new(socket: TcpStream) -> Self {
        Connection {
            socket,
            expect: HEADER_LEN,
            header_buffer: Bytes::new(),
            payload_buffer: Bytes::new(),
        }
    }

    pub fn expect(&mut self, expect: usize) {
        self.expect = expect;
    }

    pub async fn send(&mut self, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        info!("prepare to write {} bytes to remote: {:?}", data.len(), self.socket.peer_addr());
        self.socket.write_all(data).await?;
        info!("write {} bytes to remote: {:?} done.", data.len(), self.socket.peer_addr());

        Ok(())
    }

    pub async fn read(&mut self) -> Result<ReadResult, Box<dyn std::error::Error>> {
        let mut buf = vec![0u8; 32];

        // 这个loop循环是从Socket从读字节流，一次完整的读包操作
        loop {
            let n = match self.socket.read(&mut buf).await {
                Ok(n) if 0 == n => {
                    info!("read 0, {:?} connection end.", self.socket.peer_addr());
                    // todo: 连接断开， 从sessions中删除会话
                    return Ok(ReadResult::Hub);
                },
                Ok(n) => {
                    
                },
                Err(e) => {
                    error!("read failure from {:?}, {}", self.socket.peer_addr(), e);
                    return Err(Box::new(e));
                },
            };
        }
        // tokio::spawn( async move {
        //     let mut buf = [0; 1024];

        //     loop {
        //         let n = match socket.read(&mut buf).await {
        //             Ok(n) if n == 0 => {
        //                 info!("read 0, connection end.");
        //                 return;
        //             },
        //             Ok(n) => {
        //                 info!("read {} bytes from {}", n, addr);
        //                 n
        //             },
        //             Err(e) => {
        //                 error!("read failure from {}, {}", addr, e);
        //                 return;
        //             }
        //         };
        //     }
        // });
    }
}