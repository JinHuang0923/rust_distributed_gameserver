use std::{collections::HashMap, env::ArgsOs, future::Future, sync::Arc, thread};

use common::message::Message;
use tokio::sync::Mutex;

use crate::{server_context::ServerContext, world::World};

pub async fn heartbeat_monitor(
    monitor_heartbeat_last_time: Arc<Mutex<HashMap<String, i64>>>, //key是client_id,与client_map中的相同
    heartbeat: i64,
    heartbeat_world_couter: Arc<Mutex<World>>,
    server_context: Arc<Mutex<ServerContext>>,
) {
    println!("Start server heartbeat monitor");
    loop {
        //每过指定时间检查一次心跳,找出是否有客户端连接断了
        tokio::time::sleep(tokio::time::Duration::from_secs_f32(3.0)).await;
        let mut last_time = monitor_heartbeat_last_time.lock().await;
        let mut del_list = Vec::new();
        for (client_id, last_heartbeat_time) in last_time.iter() {
            let timestamp = chrono::Local::now().timestamp_millis();
            if timestamp - last_heartbeat_time > heartbeat {
                println!("client {} is offline", client_id);
                //加入需要移除心跳记录的list
                del_list.push(client_id.clone());

                //do something
                // f((client_id.to_string(), arg.clone()));
                remove_offline_client(client_id, &heartbeat_world_couter, &server_context).await;
            }
        }
        //移除心跳记录
        for client_id in del_list.iter() {
            last_time.remove(client_id);
        }
        //更新一下连接数
        heartbeat_world_couter.lock().await.connection_count = last_time.len() as u32;
    }
}
/// 心跳相关(与客户端)
#[allow(clippy::single_match)]
pub async fn remove_offline_client(
    client_id: &str,
    heartbeat_world_couter: &Arc<Mutex<World>>,
    server_context: &Arc<Mutex<ServerContext>>,
) {
    let server_ctx = server_context.lock().await;
    match server_ctx.node_type {
        common::node::NodeType::Master => {
            let mut world = heartbeat_world_couter.lock().await;

            // 更新世界 移除对应的user
            world.remove_user_by_client_id(client_id);
        }
        common::node::NodeType::Slave => {
            let mut world = heartbeat_world_couter.lock().await;

            // todo: 心跳停止要移除对应的user,如果是slave节点,需要通知master节点移除这个用户
            //1. get 对应的userid
            let client_map = world.client_map.clone();
            let user_id_opt = client_map.get(client_id);

            match user_id_opt {
                Some(user_id) => {
                    // 从连接列表移除
                    world.client_map.remove(client_id);
                    //删除后其实已经不计入connection了

                    //build a del_user msg,send to master
                    let del_user_msg =
                        Message::del_user(&server_ctx.self_node_info.get_key(), *user_id)
                            .to_zmq_dealer_msg();
                    server_ctx
                        .to_master_sender
                        .send(del_user_msg)
                        .await
                        .unwrap();
                }
                None => {
                    // debug!("can't find user_id by client_id:{},无需移除", client_id);
                }
            }
            //2. 通知master节点移除这个用户
        }
    }
}
