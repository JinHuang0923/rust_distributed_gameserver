use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]

pub struct NodeInfo {
    pub node_addr: String,
    pub main_channel_port: u16, //这个启动后不会变了
    pub heartbeat_port: u16,
    pub pub_port: u16, //pub端口 master节点才会使用到
    pub node_type: NodeType,
}
impl NodeInfo {
    //default
    pub fn default() -> Self {
        NodeInfo {
            node_addr: "".to_string(),
            main_channel_port: 0,
            heartbeat_port: 0,
            node_type: NodeType::Master,
            pub_port: 0,
        }
    }
    //既是main_addr,同时也是slave to master的client_id,以及to registry的client_id
    pub fn get_key(&self) -> String {
        format!("{}:{}", self.node_addr, self.main_channel_port)
    }
    pub fn get_pub_addr(&self) -> String {
        format!("{}:{}", self.node_addr, self.pub_port)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]

pub enum NodeType {
    Master,
    Slave,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateInfo {
    //指定的节点最大连接数
    pub max_connection: u32,
    pub current_connection: u32,
    //负载率 current_connection/max_connection
    pub load_factor: f32,
    //世界更新平均延迟(毫秒)
    pub avg_update_delay: f32,
    //当前世界人数
    pub user_count: u32,
}
impl Default for StateInfo {
    fn default() -> Self {
        StateInfo {
            max_connection: 0,
            current_connection: 0,
            load_factor: 0.9,
            avg_update_delay: 0.0,
            user_count: 0,
        }
    }
}
