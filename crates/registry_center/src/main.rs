mod app_context;
mod handler;
mod result;
mod route;
mod server_keeping;
mod zmq_handler;

use app_context::AppContext;
use common::{
    message::{Message, Operation},
    node::{NodeInfo, NodeType, StateInfo},
};
use handler::get_min_load_node;
use route::init_router;
use std::{collections::HashMap, f32::consts::E, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        Mutex,
    },
    time::sleep,
};
use tracing::{debug, error, Level};
use tracing_subscriber::FmtSubscriber;
use zeromq::{
    util::PeerIdentity, RepSocket, RouterSocket, Socket, SocketRecv, SocketSend, ZmqMessage,
};

use crate::{server_keeping::heartbeat_monitor, zmq_handler::handle_msg_task};

#[tokio::main]
async fn main() {
    // initialize tracing
    // tracing_subscriber::fmt::init();

    let subscriber = FmtSubscriber::builder()
        // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
        // will be written to stdout.
        .with_max_level(Level::DEBUG)
        // completes the builder.
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let main_socket = crate_main_socket().await;

    let (to_client_sender, mut forward_receiver) = tokio::sync::mpsc::channel(100);

    let context = Arc::new(Mutex::new(AppContext {
        node_list: HashMap::new(),
        node_state: HashMap::new(),
        master_node: None,
        rep_client_sender: to_client_sender.clone(),
        node_id_peer_id_map: HashMap::new(),
    }));

    //心跳监控
    tokio::spawn(heartbeat_monitor(context.clone()));

    //测试打印
    // tokio::spawn(test_print(context.clone()));

    //处理消息的主线程
    tokio::spawn(rep_recv_and_send_thread(
        main_socket,
        context.clone(),
        to_client_sender,
        forward_receiver,
    ));

    //开启axum web服务
    let router = init_router(context).await;
    debug!("server start");

    axum::Server::bind(&SocketAddr::from(([127, 0, 0, 1], 3090)))
        .serve(router.into_make_service())
        .await
        .unwrap();
}
/// 使用routersocket是因为会发送到不同的client,需要指定peer_id
pub async fn crate_main_socket() -> RouterSocket {
    let mut repsocket = RouterSocket::new();
    repsocket.bind("tcp://127.0.0.1:9230").await.unwrap();
    repsocket
}
pub async fn rep_recv_and_send_thread(
    mut rep_socket: RouterSocket,
    context: Arc<Mutex<AppContext>>,
    to_client_sender: Sender<ZmqMessage>,
    mut forward_receiver: Receiver<ZmqMessage>,
) {
    loop {
        tokio::select! {
                message = rep_socket.recv() => {
                    //先拆分出peer_id
                    let message = message.unwrap();

                    let peer_id = message.get(0).unwrap().clone().try_into().unwrap();
                    // let receive: String = message.try_into().unwrap();
                    //第三条是正文
                    let receive = String::from_utf8(message.into_vecdeque()[2].to_vec()).unwrap();
                    let receive_msg = serde_json::from_str::<common::message::Message>(&receive).unwrap();
                    tokio::spawn(handle_msg_task(to_client_sender.clone(), receive_msg, peer_id,context.clone()));
                }
                //要转发给客户端的消息
                wait_send_msg = forward_receiver.recv() => {
                    let send_msg = wait_send_msg.unwrap();
                    //转发给客户端
                    //todo: 我要怎么知道转发给哪个客户端呢 (把peer_id与node的client_id对应起来)

                    match rep_socket.send(send_msg).await{
                        Ok(_) => {}
                        Err(e) => {
                            error!("send to client error: {:?}",e);
                        }
                    }

                    // rep_socket.send(send_msg).await.unwrap();

                }
        }
    }
}
// pub async fn main_channel(context: Arc<Mutex<AppContext>>) {
//     debug!("main_channel start");

//     loop {
//         // recv 可能出现 "not yet implemented",catch住,报错继续循环
//         let recv_maybe_panic = async {
//             match repsocket.recv().await {
//                 Ok(receive) => {
//                     let receive: String = receive.try_into().unwrap();
//                     // println!("receive: {}", receive);
//                     let receive_msg =
//                         serde_json::from_str::<common::message::Message>(&receive).unwrap();
//                     handle_msg(&mut repsocket, receive_msg, context.clone()).await;
//                 }
//                 Err(e) => {
//                     error!("socket recv error: {:?}", e);
//                 }
//             }
//         };
//         recv_maybe_panic.await;
//         // let result = recv_maybe_panic.catch_unwind().await;
//         // if result.is_err() {
//         //     println!("catch_unwind error: {:?}", result);
//         // }
//     }
// }
pub async fn test_print(context: Arc<Mutex<AppContext>>) {
    loop {
        let context = context.lock().await;
        debug!("node list: {:?}", context.node_list);
        // println!("node_list: {:?}", context.node_list);
        // println!("node_state: {:?}", context.node_state);
        tokio::time::sleep(Duration::from_secs_f32(0.1)).await;
    }
}

// async fn update_node_resource() {}
// async fn register_node() {}
