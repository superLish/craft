use futures::join;
use crate::network::host::Host;
use crate::crypto::NodeId;
use crate::config::Config;


mod host;
mod connection;
mod session;

pub async fn start_network(config: Config) {
    let network = NetworkService::new(config);
    if let Err(e) = network.start().await {
        error!("{:?}", e);
    }
}

/// 网络服务，提供对外接口
pub struct NetworkService {
    config: Config,
    host: Host,
}

impl NetworkService {
    pub fn new(config: Config) -> Self {
        let host = Host::new(&config);
        NetworkService {
            config,
            host,
        }
    }

    /// 启动网络服务
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        // 1. 连接到种子节点
        let task1 = async {
            if let Some(ref seed) = self.config.seed {
                if let Err(e) = self.host.connect_seed(seed).await {
                    error!("{:?}", e);
                }
            }
        };

        // 2. 开启定时服务
        let task2 = async {
            self.host.timer_task().await;    
        };

        // 3. 监听
        let task3 = async {
            if let Err(e) = self.host.start(&self.config).await {
                error!("{:?}", e);
            } 
        };  

        join!(task1, task2, task3);

        Ok(())
    }


}