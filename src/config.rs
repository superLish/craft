// use crate::crypto::NodeId;

#[derive(Debug)]
pub struct Config {
    pub listen_port: u16,        // 本地监听端口
    pub secret: String,          // 本节点私钥
    // nodeid: String,          // 节点公钥ID
}

impl Default for Config {
    fn default() -> Self {
        Config {
            listen_port: 30000,
            secret: "e12aaf3b8500efcd89b7914263067b97a2d81d76f6bbc4b5765611669986b5c6".to_string(),
        }
    }
}