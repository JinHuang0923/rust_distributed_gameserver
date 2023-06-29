use std::{error::Error, future::Future, sync::Arc};

use common::{
    basic::{RectScope, User, Velocity},
    message::{peer_identity_to_string, Message, Operation},
};
use idgenerator::IdInstance;
use serde_json::error;
use tokio::sync::{
    mpsc::{self, Receiver, Sender},
    Mutex, MutexGuard,
};
use tracing::{debug, error};
use tracing_subscriber::field::debug;
use zeromq::{util::PeerIdentity, Socket, SocketRecv, SocketSend, ZmqMessage};
type UserId = i64;
use crate::{config::Mode, server_context::ServerContext, world::World};
pub enum UserType {
    NoVelocity,
    Global,
}
pub async fn handle_operation<M, S>(
    is_cluster: bool,
    is_slave: bool,
    do_in_self_world: M,
    do_in_slave: S,
) where
    M: Future,
    S: Future,
{
    if is_cluster {
        if is_slave {
            do_in_slave.await;
        } else {
            do_in_self_world.await;
        }
    } else {
        do_in_self_world.await;
    }
}

/// sender是给客户端回复消息的
/// 除了query这类查询操作,其他操作都会影响世界数据,需要转发到master处理
/// 单single模式怎么区分处理?
/// login logout需要单独做处理,因为user_id的映射关联到心跳线程
/// login 在slave端生成用户信息,拿到用户id,就可以添加到client心跳中了,然后发送添加用户的信息给master
/// logout slave模式下通过client_id找到user_id,然后删除心跳中的用户信息,client_map中的用户信息,然后发送删除用户消息的id给master,master从主世界移除
/// master得分两种情况处理login,logout,一种是客户端直连,一种是slave转发过来的(处理的是add_user,没事了)
/// todo: 写个do_login_in_self_world,以及logout
/// todo: 实现add_user,del_user的操作处理(这个是不记录心跳的)
/// todo: 响应客户端的消息(转发),是可以异步处理的,可以写一个转发的异步task_thread function,传递sender与msg进去就行了,能够优化响应速度
pub async fn handle_msg_task(
    server_ctx: Arc<Mutex<ServerContext>>,
    world: Arc<Mutex<World>>,
    // to_client_sender: &Sender<ZmqMessage>,
    mut msg: Message,
    peer_id: PeerIdentity, //todo: 这里的peer_id是客户端直连/slave到master 的peer_id,如果是master转发的话,peer_id要从消息中获取重新构建msg
                           // to_master_sender: &Sender<ZmqMessage>,
) {
    let ctx = server_ctx.lock().await;
    let is_cluster = ctx.mode == Mode::Cluster;
    let is_slave = ctx.node_type == common::node::NodeType::Slave;
    let slave_client_id = ctx.comunicacion.get_key();
    let to_client_sender = &ctx.to_client_sender;
    let to_master_sender = &ctx.to_master_sender;
    let peer_id_str = peer_identity_to_string(peer_id.clone());
    // 加入自定义msg,需要的解出来
    // msg.set_peer_id(&peer_id_str);
    // 仅 slave node 需要把消息包含的peer_id_str塞进消息里,master不需要
    match msg.operation {
        Operation::Login => {
            //三种模式下 处理login的前置操作,都是判断是否重复登录
            let mut world = world.lock().await;

            // 判断是否登录过,登陆过不处理
            if world.client_map.contains_key(&msg.client_id) {
                debug!("user has logined");
                let _ = to_client_sender
                    .send(
                        Message::resp("user has logined")
                            // .set_peer_id(&peer_id_str)
                            .to_router_msg(peer_id),
                    )
                    .await;
                return;
            }
            debug!("login! add user");

            // 添加用户流程
            // 先创建一个用户
            let user_id = IdInstance::next_id();
            let user = User::random_without_velocity(user_id);

            //记录client_id与user_id的映射(心跳那边也需要)
            world.client_map.insert(msg.client_id, user_id);

            if is_slave {
                // slave处理login没有响应,已经转为了add_user的消息发送给master了,master那边会有响应,到时候会自动转发给客户端
                //slave handle opration_login: convert to adduser msg,send to master_node
                // let ctx = server_ctx.lock().await;
                let add_user_msg = Message::add_user(&ctx.comunicacion.get_key(), user)
                    .set_peer_id(&peer_id_str)
                    .to_zmq_dealer_msg();
                //发送给master
                debug!("send add_user_msg to master");
                match to_master_sender.send(add_user_msg).await {
                    Ok(_) => {}
                    Err(e) => {
                        error!("send add_user_msg to master error:{}", e);
                    }
                }
            } else {
                //user直接添加到世界中
                debug!("add user to world");
                world.add_user(user);
                debug!("return user_id to client");

                //返回user_id to client
                // 这里set的peer_id有问题,没能成功转发到client(在slave节点下)
                to_client_sender
                    .send(
                        Message::resp(&user_id.to_string())
                            // .set_peer_id(&peer_id_str)
                            .to_router_msg(peer_id),
                    )
                    .await
                    .unwrap();
            }

            // slave模式下也需要保持client_map与user_id的对应
        }
        Operation::Logout => {
            if is_slave {
                debug!("slave logout handle....");
                //slave node handle this opt.
                //删除client_map映射
                let mut world = world.lock().await;
                //先get 对应的user_id
                let user_id = world.client_map.get(&msg.client_id).unwrap().to_owned();
                world.client_map.remove(&msg.client_id);
                //todo: 删除后其实已经不计入connection了,但是需要把心跳timemap中删除,不然会一直发送心跳

                //build a del_user msg,send to master
                let del_user_msg = Message::del_user(&ctx.comunicacion.get_key(), user_id)
                    .set_peer_id(&peer_id_str)
                    .to_zmq_dealer_msg();
                to_master_sender.send(del_user_msg).await.unwrap();
                debug!("slave logout handle end");
            } else {
                //single模式或者master节点
                let mut world = world.lock().await;
                debug!("del user");
                //从世界中登出user(同时会从client_map中删除)
                world.remove_user_by_client_id(&msg.client_id);
                to_client_sender
                    .send(
                        Message::resp("logout reply")
                            // .set_peer_id(&peer_id_str)
                            .to_router_msg(peer_id),
                    )
                    .await
                    .unwrap();
            }
        }
        Operation::AddUser => {
            if is_slave {
                error!("slave node should not receive add_user msg");
            }
            let peer_id_str_opt = msg.client_peer_id;
            let user = serde_json::from_str::<User>(&msg.content.unwrap()).unwrap();
            let user_id: i64 = user.id;
            world.lock().await.add_user(user);
            to_client_sender
                .send(
                    Message::resp(&user_id.to_string())
                        .set_peer_id_opt(peer_id_str_opt)
                        .to_router_msg(peer_id),
                )
                .await
                .unwrap();
        }
        Operation::DelUser => {
            if is_slave {
                error!("slave node should not receive add_user msg");
            }
            let peer_id_str_opt = msg.client_peer_id;

            // let user_id = serde_json::from_str::<UserId>(&msg.content.unwrap()).unwrap();
            world.lock().await.remove_user(msg.user_id.unwrap());

            to_client_sender
                .send(
                    Message::resp("del user reply")
                        .set_peer_id_opt(peer_id_str_opt)
                        .to_router_msg(peer_id),
                )
                .await
                .unwrap();
        }
        Operation::Aoe => {
            handle_operation(
                is_cluster,
                is_slave,
                do_aoe_in_self_world(&world, to_client_sender, msg.clone(), peer_id, &peer_id_str),
                do_aoe_in_slave(
                    to_master_sender,
                    msg.set_peer_id(&peer_id_str),
                    slave_client_id,
                ),
            )
            .await;
        }

        Operation::SetVelocity => {
            handle_operation(
                is_cluster,
                is_slave,
                do_set_velocity_in_self_world(
                    &world,
                    to_client_sender,
                    msg.clone(),
                    peer_id,
                    &peer_id_str,
                ),
                do_set_velocity_in_slave(
                    to_master_sender,
                    msg.set_peer_id(&peer_id_str),
                    slave_client_id,
                ),
            )
            .await;
        }
        Operation::Query => {
            // query是slave直接处理,不需要转发给master

            //todo: 这里给用户返回的数据量很多，后续要转为protobuf格式的二进制数据，暂时先用json实现
            //1. 获取参数
            //caculate response time
            let start = tokio::time::Instant::now();
            let scope = serde_json::from_str::<RectScope>(&msg.content.unwrap()).unwrap();
            let users = world.lock().await.query(scope);
            // 释放锁 todo:是否会自动释放,还是到作用域结束时才释放
            //人数
            debug!("query user count:{}", users.len());
            to_client_sender
                .send(
                    Message::query_resp(users)
                        // .set_peer_id(&peer_id_str)
                        .to_router_msg(peer_id),
                )
                .await
                .unwrap();
            debug!("query response time:{:?}", start.elapsed());
        }

        _ => {
            error!("unknown msg operation");
        }
    }
}

// todo: 自己定义的msg中需要添加一个peer_id字段,在转发的时候需要拿出来替换掉本来的响应消息中的peer_id,才能正确发送到客户端
pub async fn redirect_msg_to_master(
    to_master_sender: &Sender<ZmqMessage>,
    mut msg: Message,
    client_id: String,
) {
    //更换client_id,把msg转发给master
    msg.client_id = client_id;
    // 通过连接的socket(这里可以开一个sender)转发给master
    to_master_sender
        .send(msg.to_zmq_dealer_msg())
        .await
        .unwrap();
}

pub async fn do_aoe_in_slave(
    to_master_sender: &Sender<ZmqMessage>,
    msg: Message,
    client_id: String,
) {
    redirect_msg_to_master(to_master_sender, msg, client_id).await;
}
pub async fn do_set_velocity_in_slave(
    to_master_sender: &Sender<ZmqMessage>,
    msg: Message,
    client_id: String,
) {
    redirect_msg_to_master(to_master_sender, msg, client_id).await;
}

pub async fn do_set_velocity_in_self_world(
    world: &Arc<Mutex<World>>,
    sender: &Sender<ZmqMessage>,
    msg: Message,
    peer_id: PeerIdentity,
    peer_id_str: &String,
) {
    let peer_id_str_opt = msg.client_peer_id;

    let mut world = world.lock().await;
    // let user_id = world.client_map.get(&msg.client_id).unwrap().to_owned();

    //2.获取参数
    let velocity = serde_json::from_str::<Velocity>(&msg.content.unwrap()).unwrap();
    world.set_velocity(msg.user_id.unwrap(), velocity);
    sender
        .send(
            Message::resp("set_velocity reply")
                .set_peer_id_opt(peer_id_str_opt)
                .to_router_msg(peer_id),
        )
        .await
        .unwrap();
}
pub async fn do_aoe_in_self_world(
    world: &Arc<Mutex<World>>,
    sender: &Sender<ZmqMessage>,
    msg: Message,
    peer_id: PeerIdentity,
    peer_id_str: &String,
) {
    let peer_id_str_opt = msg.client_peer_id;

    //caculate response time
    let start = tokio::time::Instant::now();
    // 1.get user_id
    let mut world = world.lock().await;
    // let user_id = world.client_map.get(&msg.client_id).unwrap().to_owned();
    // 2. 获取参数
    let content = msg.content.unwrap();
    let mut iter = content.split_whitespace();
    let radius = iter.next().unwrap().parse::<f32>().unwrap();
    let money = iter.next().unwrap().parse::<u64>().unwrap();
    // 3. 执行aoe操作
    world.aoe(msg.user_id.unwrap(), radius, money);
    let repl = format!(
        "aoe reply,aoe response time:{}",
        start.elapsed().as_millis()
    );
    sender
        .send(
            Message::resp(&repl)
                .set_peer_id_opt(peer_id_str_opt)
                .to_router_msg(peer_id),
        )
        .await
        .unwrap();
}
pub fn add_user2_world(mut world: &mut MutexGuard<'_, World>, user_type: UserType) -> i64 {
    // world.current_id += 1;
    let id = IdInstance::next_id();
    match user_type {
        UserType::NoVelocity => {
            let user = User::random_without_velocity(id);
            world.add_user(user);
        }
        UserType::Global => {
            let user = User::random(id);
            world.add_user(user);
        }
    };
    id
}

pub async fn main_channel_thread(
    server_ctx: Arc<Mutex<ServerContext>>,
    world_counter: Arc<Mutex<World>>,
    // to_client_sender: Sender<ZmqMessage>,
    mut to_client_receiver: Receiver<ZmqMessage>,
    port: u16,
) {
    debug!("Start server main channel");
    //todo: 把rep换成router是不是可以避免并发请求时出现no request process的情况
    let mut socket = zeromq::RouterSocket::new();
    socket
        .bind(&format!("tcp://127.0.0.1:{port}"))
        .await
        .unwrap();

    // let main_socket = Arc::new(tokio::sync::Mutex::new(socket));
    //todo:这里的tx，rx需要改为接受protobuf格式的数据
    // let (to_client_sender, mut to_client_receiver): (Sender<ZmqMessage>, Receiver<ZmqMessage>) = mpsc::channel(100);
    // tokio::spawn(main_response_thread(main_socket.clone(), rx));

    //读线程
    loop {
        tokio::select! {
            message = socket.recv() =>{
                //先获取peer_id
                  //先拆分出peer_id
                  let message = message.unwrap();
                  debug!("main channel receive message:{:?}",message);

                  let peer_id = message.get(0).unwrap().clone().try_into().unwrap();
                  // let receive: String = message.try_into().unwrap();
                  //第三条是正文
                  let receive = String::from_utf8(message.into_vecdeque()[2].to_vec()).unwrap();
                  let receive_msg = serde_json::from_str::<common::message::Message>(&receive).unwrap();
                // let repl = format!("{} reply", &receive_msg.operation);
                // todo:handle_msg改为线程task异步执行,是否可以提升响应速度?
                // handle_msg_task(&server_ctx,&world_counter, receive_msg,peer_id).await;
                tokio::spawn(handle_msg_task(server_ctx.clone(),world_counter.clone(), receive_msg,peer_id));
                // socket.send(repl.into()).await.unwrap();
                //todo: 如果是集群模式并且是slave,使用另一种handle_msg的方法(转发给master)

            }
            redirect_msg = to_client_receiver.recv() =>{
                //这里可能是master给slave响应,也可能是slave给client响应
                let redirect_msg = redirect_msg.unwrap();
                debug!("to_client_sender msg:{:?}",redirect_msg);
                // 这里的msg已经有了peer_id,在slave节点上,这个peer_id应该是client的id,在master节点上,这个peer_id应该是slave的peer_id
                match socket.send(redirect_msg).await{
                    Ok(_)=>{},
                    Err(e)=>{
                        // bug: send msg error:Destination client not found by identity,可能是peer_id的问题,应该是logout出现的.
                        error!("send msg error:{}",e);
                    }
                }

            }
        }
    }

    // loop {
    //     // main_socket.lock().await.
    //     let receive: String  = receive.try_into().unwrap();
    //     println!("Received request: {}", receive);
    //     let receive_msg = serde_json::from_str::<common::message::Message>(&receive).unwrap();
    //     // let repl = format!("{} reply", &receive_msg.operation);
    //     handle_msg(world_counter.clone(), &tx, receive_msg);
    //     // socket.send(repl.into()).await.unwrap();
    // }
}
// async fn main_response_thread(
//     response_socket: Arc<tokio::sync::Mutex<zeromq::RepSocket>>,
//     receiver:Receiver<String>,
// ) {
//     // loop {
//         let receive = receiver.recv().();
//         println!("Received request: {}", receive);
//         //转发响应给客户端
//         response_socket.lock().await.send(receive.into()).await.unwrap();
//     // }
// }
