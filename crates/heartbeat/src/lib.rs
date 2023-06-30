use std::{collections::HashMap, env::ArgsOs, future::Future, sync::Arc, thread};

use common::message::Message;
use tokio::sync::{
    mpsc::{Receiver, Sender},
    Mutex,
};
use zeromq::{RepSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

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

pub async fn heartbeat_update(
    heartbeat_last_time_map: Arc<Mutex<HashMap<String, i64>>>,
    port: u16,
) {
    // hb_transfer_station_thread(heartbeat_last_time, port).await;
    println!("Start server heartbeat channel");
    let mut hb_socket = zeromq::RepSocket::new();
    hb_socket
        .bind(&format!("tcp://127.0.0.1:{port}"))
        .await
        .unwrap();
    loop {
        let zmq_msg = hb_socket.recv().await.unwrap();
        // handle_hb_msg_task(msg,heartbeat_last_time.clone()).await;
        let receive: String = zmq_msg.try_into().unwrap();
        let ping_msg = serde_json::from_str::<common::message::Message>(&receive).unwrap();

        let timestamp = chrono::Local::now().timestamp_millis();
        heartbeat_last_time_map
            .lock()
            .await
            .insert(ping_msg.client_id.clone(), timestamp);
        hb_socket
            .send(Message::pong(&ping_msg.client_id).to_rep_msg())
            .await
            .unwrap();
    }
}
pub async fn handle_hb_msg_task(
    zmq_msg: ZmqMessage,
    heartbeat_last_time_map: Arc<Mutex<HashMap<String, i64>>>,
    pong_sender: Sender<ZmqMessage>,
) {
    let receive: String = zmq_msg.try_into().unwrap();
    let ping_msg = serde_json::from_str::<common::message::Message>(&receive).unwrap();

    let timestamp = chrono::Local::now().timestamp_millis();
    heartbeat_last_time_map
        .lock()
        .await
        .insert(ping_msg.client_id.clone(), timestamp);
    match pong_sender
        .send(Message::pong(&ping_msg.client_id).to_rep_msg())
        .await
    {
        Ok(_) => {}
        Err(_) => {
            println!("send pong msg error")
        }
    }
}
pub async fn hb_transfer_station_thread(
    heartbeat_last_time_map: Arc<Mutex<HashMap<String, i64>>>,
    port: u16,
) {
    println!("Start server heartbeat channel");
    let mut hb_socket = zeromq::RepSocket::new();
    hb_socket
        .bind(&format!("tcp://127.0.0.1:{port}"))
        .await
        .unwrap();
    let (pong_sender, mut pong_receiver) = tokio::sync::mpsc::channel::<ZmqMessage>(100);

    loop {
        tokio::select! {
            msg_or_err = hb_socket.recv() => {
                if let Ok(msg) = msg_or_err {
                    tokio::spawn(handle_hb_msg_task(msg.clone(),heartbeat_last_time_map.clone(),pong_sender.clone()));
                }
            }
            msg = pong_receiver.recv() => {
                hb_socket.send(msg.unwrap()).await.unwrap();
            }
        }
    }
}
