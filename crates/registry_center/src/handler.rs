use std::{collections::HashMap, hash::Hash, sync::Arc};

use axum::{extract::State, http::StatusCode, Json};
use common::node::NodeInfo;
use serde::__private::de;
use tokio::sync::Mutex;
use tracing::debug;

use crate::{
    app_context::AppContext, result::Resp, server_keeping::NodeState,
    zmq_handler::get_master_node_addr,
};
pub async fn node_states(
    State(app_context): State<Arc<Mutex<AppContext>>>,
) -> Resp<HashMap<String, NodeState>> {
    let node_state_map = app_context.lock().await.node_state.clone();
    Resp::success(node_state_map)
}
pub async fn nodes(
    State(app_context): State<Arc<Mutex<AppContext>>>,
) -> (StatusCode, Json<Vec<NodeInfo>>) {
    // to_vec
    let node_list = app_context
        .lock()
        .await
        .node_list
        .values()
        .cloned()
        .collect::<Vec<NodeInfo>>();

    (StatusCode::CREATED, Json(node_list))
}
pub async fn get_master(State(app_context): State<Arc<Mutex<AppContext>>>) -> Resp<NodeInfo> {
    let master_addr = app_context.lock().await.master_node.clone();

    if let Some(addr) = master_addr {
        return Resp::success(addr);
    }
    // (StatusCode::CREATED, Json(master_addr))
    Resp::fail(NodeInfo::default())
        .code(500)
        .message("no available master node")
}
/// 获取负载均衡的节点信息(for client)
pub async fn get_load_banlance_connection(
    State(ctx): State<Arc<Mutex<AppContext>>>,
) -> Resp<NodeInfo> {
    debug!("get_load_banlance_connection");

    let mut node_state_list = ctx.lock().await.node_state.clone();
    let node_list = ctx.lock().await.node_list.clone();
    // < 1 无可用节点
    if node_list.is_empty() {
        debug!("no available server node");
        return return_no_available_server_node().await;
    }
    //单节点的话直接返回这个,不用管是否是master
    if node_list.len() == 1 {
        let node_info = node_list.values().next().unwrap();
        debug!("only one node,return it {:?}", node_info);
        return Resp::success(node_info.clone());
    }

    //获取最小负载量的非master节点,先移除master节点 from 待选节点列表(大于1个节点,master节点必然存在,注册时已经保证了)
    let master_node_key = ctx.lock().await.master_node.clone().unwrap().get_key();
    node_state_list.remove(&master_node_key);

    let node_info = get_min_load_node(node_state_list.clone(), node_list).await;

    match node_info {
        Some(node) => {
            //最小负载的这个节点不能是满的
            if node_state_list
                .get(&node.get_key())
                .unwrap()
                .state_info
                .load_factor
                < 1.0
            {
                debug!("get_min_load_node success {:?}", node);
                Resp::success(node.clone())
            } else {
                return_no_available_server_node().await
            }
        }
        None => return_no_available_server_node().await,
    }
}
pub async fn return_no_available_server_node() -> Resp<NodeInfo> {
    Resp::fail(NodeInfo::default())
        .code(500)
        .message("no available server node")
}
#[allow(clippy::comparison_chain)]
pub async fn get_min_load_node(
    node_state_list: HashMap<String, NodeState>,
    node_list: HashMap<String, NodeInfo>,
) -> Option<NodeInfo> {
    if node_list.is_empty() {
        return None;
    }
    //负载均衡调用这个传进来的list已经去除了master,所以先获取第一个作为默认的
    // let (mut node_key, mut min_state) = (node_l, None);
    let mut min_load_node_key = node_list.keys().next().unwrap().clone();
    let mut min_load_state_info = node_state_list.get(&min_load_node_key).unwrap().clone();
    debug!("first min_load_state_info {:?}", min_load_state_info);
    dbg!(node_state_list.clone());

    //获取负载量最小的节点
    for (node_key, state) in node_state_list {
        if state.state_info.load_factor < min_load_state_info.state_info.load_factor {
            min_load_node_key = node_key;
            min_load_state_info = state;
        } else if state.state_info.load_factor == min_load_state_info.state_info.load_factor {
            //负载率相同,比较最大连接数
            if state.state_info.max_connection > min_load_state_info.state_info.max_connection {
                min_load_node_key = node_key;
                min_load_state_info = state;
            } else if state.state_info.max_connection
                == min_load_state_info.state_info.max_connection
            {
                //最大连接数也相同,说明这是两个相同配置的节点,两个都一样,但是为了防止精度误差(负载率是小数,可能有很小的误差),比较一下两个节点有没有某个连接数更少
                if state.state_info.current_connection
                    < min_load_state_info.state_info.current_connection
                {
                    min_load_node_key = node_key;
                    min_load_state_info = state;
                }
            }
        }
    }
    //获取到最小负载量的节点信息
    node_list.get(&min_load_node_key).cloned()
}
