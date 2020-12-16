use crate::network::host::Host;
use crate::crypto::NodeId;
use crate::config::Config;
use crate::network::connection::Bytes;
use tokio::sync::mpsc;

mod host;
mod connection;
mod session;
mod error;

pub use host::enode_str_parse;


pub async fn start_network(config: Config, tx_server_event: mpsc::Sender<ServerEvent>, rx_server_event: mpsc::Receiver<ServerEvent>, tx_net_event: mpsc::Sender<NetEvent>) {
    let mut network = NetworkService::new(config, rx_server_event, tx_net_event);
    if let Err(e) = network.start().await {
        error!("{:?}", e);
    }

    info!("network service end.");
}

/// 网络服务
pub struct NetworkService {
    config: Config,
    host: Host,
    receiver: mpsc::Receiver<ServerEvent>,
}

impl NetworkService {
    pub fn new(config: Config, receiver: mpsc::Receiver<ServerEvent>, net_sender: mpsc::Sender<NetEvent>) -> Self {
        let host = Host::new(&config, net_sender);
        NetworkService {
            config,
            host,
            receiver,
        }
    }

    /// 启动网络服务
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        while let Some(event) = self.receiver.recv().await {
            info!("network service recv {:?}", event);
            match event {
                ServerEvent::Start => {
                    if let Err(e) = self.host.start(&self.config).await {
                        error!("{:?}", e);
                        // fimxe: 错误处理待补充
                    }
                },
                ServerEvent::Send(ref data) => {
                    if let Err(e) = self.host.send_packet(data).await {
                        error!("{:?}", e);
                    }
                },
                ServerEvent::ActiveConnect(ref seed) => {
                    if let Err(e) = self.host.connect_seed(seed).await {
                        error!("{:?}", e);
                        // fixme: 错误处理待补充,比如，重试连接等处理，这里暂不实现
                    }
                }
            }
        }

        Ok(())
    }
}

// 对外事件
#[derive(Debug)]
pub enum NetEvent {
    Connected(NodeId),      // 新的连接
    Read(NetPacket),        // 网络读
    Disconnected(NodeId),   // 连接已断开
}

// 内部事件
#[derive(Debug)]
pub enum ServerEvent {
    Start,                      // 启动本地监听
    ActiveConnect(String),      // 主动发起连接
    Send(NetPacket),            // 向某节点发送数据
    // Broadcast(Bytes)         // todo: 广播，暂不实现
}

#[derive(Debug)]
pub struct NetPacket {
    nodeid: NodeId,
    data: Bytes,
}

impl NetPacket {
    pub fn new(nodeid: NodeId, data: &Bytes) -> Self {
        NetPacket {
            nodeid,
            data: data.clone(),
        }
    }
}