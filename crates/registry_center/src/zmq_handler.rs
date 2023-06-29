use std::sync::Arc;

use common::{
    message::{Message, Operation, RegisterResp},
    node::{NodeInfo, NodeType, StateInfo},
};
use tokio::sync::{mpsc::Sender, Mutex};
use tracing::{debug, error};
use tracing_subscriber::field::debug;
use zeromq::{util::PeerIdentity, RepSocket, RouterSocket, SocketSend, ZmqMessage};

use crate::{app_context::AppContext, server_keeping::NodeState};
pub async fn get_master_node_addr(context: &Arc<Mutex<AppContext>>) -> Option<String> {
    for (addr, node_info) in &context.lock().await.node_list {
        if node_info.node_type == NodeType::Master {
            return Some(addr.to_string());
        }
    }
    None
}
/// 消费速度太慢了,客户端0.02s发一次,处理完所有消息 34s之后了,开线程处理这个试试(可能是因为ctx的锁?)
/// 使用线程后,处理消息还是有3s左右的延迟,不过已经可以接受了
pub async fn handle_msg_task(
    to_client_sender: Sender<ZmqMessage>,
    msg: Message,
    peer_id: PeerIdentity,
    context: Arc<Mutex<AppContext>>,
) {
    match msg.operation {
        Operation::Register => {
            if context.lock().await.node_list.get(&msg.client_id).is_some() {
                let msg = Message::register_resp(RegisterResp::fail("already register"))
                    .to_router_msg(peer_id);
                //已经注册过了,回复消息
                // repsocket.send("already register".into()).await.unwrap();
                to_client_sender.send(msg).await.unwrap();
                return;
            }
            // 注册流程
            if let Some(content) = msg.content {
                let node_info = serde_json::from_str::<NodeInfo>(&content).unwrap();
                //if register node is master type,verify master node not exist
                //拆分消息第一帧获取peer_id

                //处理必要的部分
                match node_info.node_type {
                    NodeType::Master => {
                        if context.lock().await.master_node.is_some() {
                            let zmq_msg = Message::register_resp(RegisterResp::fail(
                                "only one master node can exist",
                            ))
                            .to_router_msg(peer_id);
                            //已经存在master节点,回复消息
                            to_client_sender.send(zmq_msg).await.unwrap();
                            return;
                        } else {
                            //更新master_node信息
                            context.lock().await.master_node = Some(node_info.clone());
                        }
                    }
                    NodeType::Slave => {
                        //slave节点只能在有master节点的情况下注册
                        if context.lock().await.master_node.is_none() {
                            //不存在master节点,暂时不能注册
                            let zmq_msg = Message::register_resp(RegisterResp::fail(
                                "master node not exist,can't register slave",
                            ))
                            .to_router_msg(peer_id);
                            to_client_sender.send(zmq_msg).await.unwrap();
                            return;
                        }
                    }
                }

                let mut context = context.lock().await;
                context
                    .node_list
                    .insert(msg.client_id.clone(), node_info.clone());
                //状态列表也需要加入
                context
                    .node_state
                    .insert(msg.client_id.clone(), NodeState::default());

                //注册成功添加client_id -> peer_id的映射
                context
                    .node_id_peer_id_map
                    .insert(msg.client_id.clone(), peer_id.clone());
                let zmq_msg =
                    Message::register_resp(RegisterResp::success()).to_router_msg(peer_id);
                debug!(
                    "register a node {},type:{:?}",
                    node_info.get_key(),
                    node_info.node_type
                );
                to_client_sender.send(zmq_msg).await.unwrap();
            }
        }

        Operation::Report => {
            // 拿到stateinfo
            match msg.content {
                Some(content) => {
                    // report 同时也是心跳消息
                    let state_info = serde_json::from_str::<StateInfo>(&content).unwrap();

                    let mut context = context.lock().await;
                    //正常情况下这里一定是get到的(todo:get不到说明被移除了)

                    if let Some(node_state) = context.node_state.get_mut(&msg.client_id) {
                        node_state.last_heartbeat_time = chrono::Local::now().timestamp_millis();
                        node_state.state_info = state_info;
                        
                        // 不回心跳消息,不然太多了server不好处理
                        // debug!("node report {}",msg.client_id);
                        // let zmq_msg = Message::resp("report success").to_router_msg(peer_id);

                        // // let _ = repsocket.send("report success".into()).await;
                        // to_client_sender.send(zmq_msg).await.unwrap();
                    } else {
                        // error!("node report but not register {}",msg.client_id);
                    }
                    // let node_state = NodeState {
                    //     last_heartbeat_time: chrono::Local::now().timestamp_millis(),
                    //     state_info,
                    // };
                    // context.node_state.insert(msg.client_id.clone(), node_state);
                }
                None => {
                    //todo: 回复非法消息
                    error!("非法report信息")
                }
            }
        }
        _ => {
            error!("unknown operation");
            // todo:为了保证一发一回,这里回复非法操作信息
        }
    }
}
