use std::{sync::Arc, time::Duration};

use common::{
    message::Message,
    node::{NodeType, StateInfo},
};
use serde::Serialize;
use tokio::{
    sync::{Mutex, MutexGuard},
    time::sleep,
};
use tracing::{debug, error};

use crate::{app_context::AppContext, handler::get_min_load_node};

///维护节点状态
#[derive(Debug, Clone, Serialize)]
pub struct NodeState {
    pub last_heartbeat_time: i64,
    pub state_info: StateInfo,
}
impl NodeState {
    pub fn default() -> Self {
        NodeState {
            last_heartbeat_time: chrono::Local::now().timestamp_millis(),
            state_info: StateInfo::default(),
        }
    }
}

pub async fn heartbeat_monitor(context: Arc<Mutex<AppContext>>) {
    sleep(Duration::from_secs(3)).await;
    debug!("Start server heartbeat monitor");
    loop {
        //每过指定时间检查一次心跳,找出是否有客户端连接断了
        let mut context = context.lock().await;
        let mut del_list = Vec::new();
        let mut need_elect_master = false;

        context
            .node_state
            .iter()
            .for_each(|(client_id, node_state)| {
                let timestamp = chrono::Local::now().timestamp_millis();
                if timestamp - node_state.last_heartbeat_time > 5000 {
                    debug!(
                        "node {} is offline,currenct_timestamp: {},last heartbeat time:{}",
                        client_id, timestamp, node_state.last_heartbeat_time
                    );
                    //do something
                    del_list.push(client_id.clone());
                    // if context.node_list.get(client_id).unwrap().node_type == NodeType::Master {
                    //     need_elect_master = true;
                    // }
                    if let Some(master_node) = &context.master_node {
                        if master_node.get_key() == *client_id {
                            need_elect_master = true;
                        }
                    }
                }
            });

        //移除挂掉节点的心跳记录与nodeInfo
        for client_id in del_list.iter() {
            context.node_state.remove(client_id);
            context.node_list.remove(client_id);
        }
        //如果需要选举master
        if need_elect_master {
            debug!("need elect master");
            //得清除master_node
            context.master_node = None;
            //todo: elect_new_master

            elect_master(&mut context).await;
        }

        tokio::time::sleep(Duration::from_secs_f32(1.0)).await;
    }
}
pub async fn elect_master(context: &mut MutexGuard<'_, AppContext>) {
    //从存活的节点中选择负载最小的节点作为master

    //这里一定是已经不存在master节点了 所以不用判断master_node是否存在
    let node_state_list = context.node_state.clone();
    let mut node_list = context.node_list.clone();

    let new_master_node_info = get_min_load_node(node_state_list, node_list.clone()).await;

    match new_master_node_info {
        Some(new_master_node) => {
            //1.更新master_node todo:这里不更新master_node,要确保收到转换的节点回复的change_master响应成功再做修改,否则会影响心跳,重复选举
            // context.master_node = Some(new_master_node.clone());
            debug!("elect new master node is {:?},wait node finish it...", new_master_node);
            //2.向master_node发送主从转换指令
            let master_client_id = new_master_node.get_key();
            // 获取对应的peer_id
            let peer_id = context
                .node_id_peer_id_map
                .get(&master_client_id)
                .expect("no peer_id found");
            let change_master_msg =
                Message::convert_to_master(&master_client_id).to_router_msg(peer_id.clone());
            //发送
            context
                .rep_client_sender
                .send(change_master_msg)
                .await
                .unwrap();

            //3. 向其他slave节点发送change_master指令
            //node_list去除这个master_node
            node_list.remove(&master_client_id);
            //遍历node_list,向其他slave节点发送change_master指令
            for (client_id, _) in node_list.iter() {
                // 获取对应的peer_id
                let peer_id = context
                    .node_id_peer_id_map
                    .get(client_id)
                    .expect("no peer_id found");
                let change_master_msg =
                    Message::change_master(&master_client_id, new_master_node.clone())
                        .to_router_msg(peer_id.clone());
                //发送
                context
                    .rep_client_sender
                    .send(change_master_msg)
                    .await
                    .unwrap();
            }
        }
        None => {
            error!("no node can be master,no any node alive");
        }
    }
}
