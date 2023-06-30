use common::node::{NodeInfo, NodeType};
use tokio::sync::mpsc::{Receiver, Sender};
use zeromq::{PubSocket, Socket, ZmqMessage};

use crate::{config::{GameServerConf, Mode}, distribute::WorldSyncDTO};

//Server运行时上下文,在运行时可能会发生变化
pub struct ServerContext {
    pub mode: Mode, //运行模式,单机模式或者集群模式,默认为单机模式,读取完config后可能会变成集群模式
    pub self_node_info: NodeInfo, //自身的节点信息
    pub node_type: NodeType,
    pub master_node: NodeInfo, //master节点的地址以及端口信息

    ///发布世界同步信息,only master,receive from world,send to slave node via pubsocket.
    pub world_sync_sender: Sender<WorldSyncDTO>, //发布世界同步信息的端口,仅为master节点使用(todo:可能不可以存这里)

    ///通过这个sender,把要发送给client的消息传给to_client_socket,
    /// 一个sender是handle_msg时使用的,clone的sender是集群模式下,slave把从master获得的rep转发给client
    pub to_client_sender: Sender<ZmqMessage>,
    ///接收来自于client的消息,转发给master_node,only for slave node,只会有一个发送方使用到这个sender
    pub to_master_sender: Sender<ZmqMessage>,
    pub to_registry_center_sender: Sender<ZmqMessage>, //发送给注册中心消息使用的sender
}
impl ServerContext {
    //default
    pub fn default() -> Self {
        let (to_client_sender, _) = tokio::sync::mpsc::channel::<ZmqMessage>(100);
        let (to_master_sender, _) = tokio::sync::mpsc::channel::<ZmqMessage>(100);
        let (world_sync_sender, _) = tokio::sync::mpsc::channel::<WorldSyncDTO>(100);
        let (to_registry_center_sender, _) = tokio::sync::mpsc::channel::<ZmqMessage>(100);
        ServerContext {
            self_node_info: NodeInfo::default(),
            node_type: NodeType::Master,
            master_node: NodeInfo::default(),
            world_sync_sender,
            to_client_sender,
            to_master_sender,
            mode: Mode::Single,
            to_registry_center_sender,
        }
    }
    pub fn new_by_config(conf: GameServerConf) -> Self {
        let (to_client_sender, _) = tokio::sync::mpsc::channel::<ZmqMessage>(100);
        let (to_master_sender, _) = tokio::sync::mpsc::channel::<ZmqMessage>(100);
        let (world_sync_sender, _) = tokio::sync::mpsc::channel::<WorldSyncDTO>(100);
        let (to_registry_center_sender, _) = tokio::sync::mpsc::channel::<ZmqMessage>(100);

        ServerContext {
            self_node_info: conf.node.clone(),
            node_type: conf.node.node_type.clone(),
            master_node: conf.node.clone(),
            world_sync_sender,
            to_client_sender,
            to_master_sender,
            mode: conf.app.mode,
            to_registry_center_sender,
        }
    }
    pub fn is_cluster(&self) -> bool {
        self.mode == Mode::Cluster
    }
}
// pub struct NodeInfo{

// }
