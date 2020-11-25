use tokio::net::TcpListener;
// use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::prelude::*;

pub struct Host {

}

impl Host {
    pub fn new() -> Self {
        Host {

        }
    }

    /// 启动网络服务， 1. 开启监听；
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("0.0.0.0:30000").await?;
        info!("listening on 0.0.0.0:30000");
        loop {
            let (mut socket, addr) = listener.accept().await?;
            info!("accept tcp connection {}", addr);

            tokio::spawn( async move {
                let mut buf = [0; 1024];

                loop {
                    let n = match socket.read(&mut buf).await {
                        Ok(n) if n == 0 => {
                            info!("read 0, connection end.");
                            return;
                        },
                        Ok(n) => {
                            info!("read {} bytes from {}", n, addr);
                            n
                        },
                        Err(e) => {
                            error!("read failure from {}, {}", addr, e);
                            return;
                        }
                    };

                    if let Err(e) = socket.write_all(&buf[0..n]).await {
                        error!("failed to write to {}, {}", addr, e);
                        return;
                    }
                    info!("write bytes to {}", addr);
                }
            });
        }


        Ok(())
    }

}