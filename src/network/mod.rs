use crate::network::host::Host;
use crate::crypto::NodeId;
use crate::config::Config;


mod error;
mod host;
mod connection;
mod session;


// pub type NodeId = usize;


/// 网络服务，提供对外接口
pub struct NetworkService {
    config: Config,
    host: Host,
}

impl NetworkService {
    pub fn new(config: Config) -> Self {
        NetworkService {
            config,
            host: Host::new(),
        }
    }

    /// 启动网络服务
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.host.start(&self.config).await
    }
}