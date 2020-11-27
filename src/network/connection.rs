use tokio::net::TcpStream;

const HEADER_LEN: usize = 16;

pub struct Connection {
    socket: TcpStream,
}

impl Connection {
    pub fn new(socket: TcpStream) -> Self {
        Connection {
            socket
        }
    }
}