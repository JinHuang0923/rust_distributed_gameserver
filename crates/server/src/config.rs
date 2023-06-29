use serde::Deserialize;
use common::node::{NodeType, NodeInfo};

#[derive(Debug, Clone, Deserialize)]
pub struct App {
    // pub name: Option<String>,
    // pub node_type: Option<NodeType>,
    pub mode: Mode,
}
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Basic {
    pub tick_time: f32,
    pub heartbeat: i64,
    pub max_aoe_radius: f32,
    pub max_connection: u32,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Registry {
    pub addr: String,
    pub port: u16,
}

// #[derive(Debug, Clone, Deserialize)]
// pub struct Comunicacion {
//     pub node_addr: String,
//     pub main_channel_port: u16,
//     pub heartbeat_port: u16,
//     pub pub_port: Option<u16>,
// }
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub enum Mode {
    Single,
    Cluster,
}
#[derive(Debug, Clone, Deserialize)]
pub struct OverloadTest {
    pub enable: bool,
    pub max_user_count: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnotherTest {
    pub enable: bool,
}
/**
* [test.overload_test]
enable = true
max_user_count = 6000
[test.another_test]
enable = false
*/
#[derive(Debug, Clone, Deserialize)]
pub struct Test {
    pub overload_test: OverloadTest,
    pub another_test: AnotherTest,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GameServerConf {
    pub app: App,
    pub basic: Basic,
    pub node: NodeInfo,
    pub registry: Registry,
    pub test: Test,
}
