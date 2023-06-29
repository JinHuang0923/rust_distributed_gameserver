use std::collections::HashMap;

use common::node::NodeInfo;
use tokio::sync::mpsc::Sender;
use zeromq::ZmqMessage;

use crate::server_keeping::NodeState;

pub struct AppContext {
    pub node_list: HashMap<String, NodeInfo>,
    pub node_state: HashMap<String, NodeState>, //维护节点状态
    pub master_node: Option<NodeInfo>,                    //当前的master节点地址/node_key
    pub rep_client_sender: Sender<ZmqMessage>,
    pub node_id_peer_id_map: HashMap<String, zeromq::util::PeerIdentity>, //node_id(前面的节点主地址key) -> peer_id(zmq的端)
}