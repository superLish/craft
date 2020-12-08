use std::fmt;

#[derive(Debug)]
pub enum NetError {
    ProtocolMismatch(String),       // 协议不匹配
    IoError(String),                // IO错误
    DataFault(String),              // 数据错误
    Unknown,
}

impl std::error::Error for NetError {
    fn description(&self) -> &str {
        match self {
            NetError::ProtocolMismatch(reason) => {
                reason
            },
            NetError::IoError(reason) => {
                reason
            },
            NetError::DataFault(reason) => {
                reason
            }
            NetError::Unknown => {
                "unknown"
            }
        }
    }
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetError::ProtocolMismatch(reason) => {
                write!(f, "protocol mismatch: {}", reason)
            },
            NetError::IoError(reason) => {
                write!(f, "io error: {}", reason)
            },
            NetError::DataFault(reason) => {
                write!(f, "data fault: {}", reason)
            }
            NetError::Unknown => {
                write!(f, "unknown error.")
            }
        }
    }
}