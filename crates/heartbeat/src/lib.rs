use std::{collections::HashMap, env::ArgsOs, future::Future, sync::Arc, thread};

use tokio::sync::Mutex;
use zeromq::{Socket, SocketRecv, SocketSend};

// 心跳监控 定时更新连接状态
/// TODO: 因为不知道future怎么重复执行并且给不同的参数,这里暂时没法复用,后续再优化
///
#[cfg(False)]
pub async fn heartbeat_monitor<T>(
    monitor_heartbeat_last_time: Arc<Mutex<HashMap<String, i64>>>,
    heartbeat: i64,
    // heartbeat_world_couter: Arc<Mutex<World>>,
    f: fn((String, T)),
    arg: T,
) where
    T: Clone,
{
    println!("Start server heartbeat monitor");
    loop {
        //每过指定时间检查一次心跳,找出是否有客户端连接断了
        tokio::time::sleep(tokio::time::Duration::from_secs_f32(5.0)).await;
        let mut last_time = monitor_heartbeat_last_time.lock().await;
        let mut del_list = Vec::new();
        for (client_id, last_heartbeat_time) in last_time.iter() {
            let timestamp = chrono::Local::now().timestamp_millis();
            if timestamp - last_heartbeat_time > heartbeat {
                println!("client {} is offline", client_id);
                //加入需要移除心跳记录的list
                del_list.push(client_id.clone());

                //do something
                f((client_id.to_string(), arg.clone()));
            }
        }
        //移除心跳记录
        for client_id in del_list.iter() {
            last_time.remove(client_id);
        }
    }
}

pub async fn heartbeat_update(heartbeat_last_time: Arc<Mutex<HashMap<String, i64>>>, port: u16) {
    println!("Start server heartbeat channel");
    let mut heartbeat_socket = zeromq::RepSocket::new();
    heartbeat_socket
        .bind(&format!("tcp://127.0.0.1:{port}"))
        .await
        .unwrap();
    loop {
        //recv老報錯导致panic 线程终止，可能得把zmq换成传统的tcp socket来做心跳检测
        match heartbeat_socket.recv().await {
            Ok(receive) => {
                let receive: String = receive.try_into().unwrap();
                let ping_msg = serde_json::from_str::<common::message::Message>(&receive).unwrap();

                let timestamp = chrono::Local::now().timestamp_millis();
                heartbeat_last_time
                    .lock()
                    .await
                    .insert(ping_msg.client_id.clone(), timestamp);

                heartbeat_socket
                    .send(
                        serde_json::to_string(&common::message::Message::pong(&ping_msg.client_id))
                            .unwrap()
                            .into(),
                    )
                    .await
                    .unwrap();
            }
            Err(e) => {
                println!("heartbeat socket recv error: {:?}", e);
                continue;
            }
        }
    }
}
