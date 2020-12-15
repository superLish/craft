

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_addr: String,      // 本地监听地址
    pub secret: String,           // 本节点私钥
    pub seed: Option<String>,             // 种子节点，连接到远端
}

impl Default for Config {
    fn default() -> Self {
        Config {
            listen_addr: "0.0.0.0:30000".to_string(),
            secret: "e12aaf3b8500efcd89b7914263067b97a2d81d76f6bbc4b5765611669986b5c6".to_string(),
            seed: None,
        }
    }
}