use std::{collections::HashMap, str::FromStr, sync::Arc};

use common::{
    basic::User,
    message::{peer_id_str_to_peer_identity, Message, Operation, RegisterResp},
    node::{NodeInfo, NodeType, StateInfo},
};
use common_server::map_tool::MapTag;
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        Mutex,
    },
};
use tracing::{debug, error, info};

use zeromq::{
    util::PeerIdentity, DealerSocket, PubSocket, Socket, SocketOptions, SocketRecv, SocketSend,
    SubSocket, ZmqMessage,
};

use crate::{
    config::{GameServerConf, Registry},
    http_rpc,
    server_context::{ServerContext},
    world::{Map, World},
};
type ClientId = String;
type ClusterRequired = ClientId;
type UserId = i64;
type MapTagStr = String;
type Maps = HashMap<MapTag, Map>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldSyncDTO {
    pub data: HashMap<MapTagStr, HashMap<UserId, User>>,
    pub timestamp: Option<i64>,
}
impl WorldSyncDTO {
    pub fn to_zmq_msg(&self) -> ZmqMessage {
        serde_json::to_string(self).unwrap().into()
    }
    pub fn from_maps(maps: Maps) -> Self {
        let mut data = HashMap::new();
        for (tag, map) in maps {
            data.insert(tag.to_string(), map.scenes.users);
        }

        Self {
            data,
            timestamp: None,
        }
    }
    pub fn user_count(&self) -> usize {
        let mut count = 0;
        for (_, users) in self.data.iter() {
            count += users.len();
        }
        count
    }
}

pub async fn connect_to_registry_center_and_register_node(
    registry_conf: Registry,
    node_info: NodeInfo,
    node_type: NodeType,
) -> DealerSocket {
    let mut socket2register = zeromq::DealerSocket::new();
    info!("wait for connect to registry...");
    socket2register
        .connect(&format!(
            "tcp://{}:{}",
            registry_conf.addr, registry_conf.port
        ))
        .await
        .unwrap();
    info!("connect to registry center success");
    //发送一条注册消息
    let msg = Message::register(node_info, node_type);
    socket2register.send(msg.to_zmq_dealer_msg()).await.unwrap();
    // recv for make sure register success
    // let recv_msg = socket2register.recv().await.unwrap();
    // println!("register msg: {:?}", recv_msg);
    socket2register
}

//集群模式下需要向注册中心报告负载率(非集群模式不报告),使用线程异步执行
/// 这里面的值也是不会变的
/// 这里进行读写分离,dealersocket一方面report,recv到的消息可能会是注册中心发布的指令
pub async fn try_report_load_factor_task(
    to_registry_center_sender: Sender<ZmqMessage>,
    client_id: String,
    state_info: StateInfo,
) {
    let report_zmq_msg = Message::state_report(&client_id, state_info).to_zmq_dealer_msg();
    match to_registry_center_sender.send(report_zmq_msg).await {
        Ok(_) => {}
        Err(_) => {
            // forward_server_to_registry_center_thread 那边挂了,注册中心也应该挂了,程序退出
            error!("to_registry_center_sender error registry server is down");
            std::process::exit(1);
        }
    }
}
/// 这个线程不会停止或是重启,以cluster模式启动都会启动这个线程
/// 集群模式下永不停止的线程
pub async fn forward_server_to_registry_center_thread(
    server_ctx: Arc<Mutex<ServerContext>>,
    mut to_registry_center_socket: DealerSocket,
    mut to_registry_center_receiver: Receiver<ZmqMessage>,
) {
    info!("forward_server_to_registry_center_thread start");
    loop {
        tokio::select! {
            recv_msg = to_registry_center_socket.recv() => {
                // 这是注册中心的响应,也可能是注册中心主动发的指令(convert_to_master,change_master)
                let recv_msg = recv_msg.unwrap();
                //handle_response
                tokio::spawn(handle_resp_from_registry_center_task(recv_msg,server_ctx.clone()));
            }
            to_registery_center_state_report = to_registry_center_receiver.recv() => {
                let to_registery_center_state_report = to_registery_center_state_report.unwrap();
                match to_registry_center_socket.send(to_registery_center_state_report).await{
                    Ok(_) => {}
                    Err(e) => {
                        //注册中心挂掉的情况现在不考虑,直接程序退出
                        error!("registry server is down:{:?}", e);
                        // process::exit(1);
                        std::process::exit(1);
                    }
                }
            }
        }
    }
}
pub async fn handle_resp_from_registry_center_task(
    msg: ZmqMessage,
    server_ctx: Arc<Mutex<ServerContext>>,
) {
    let msg = Message::from_zmq_reply_msg(msg);
    match msg.operation {
        Operation::RegisterResp => {
            // 注册是否成功,失败程序应该异常退出(集群模式下必须与注册中心建立链接)
            let register_resp =
                serde_json::from_str::<RegisterResp>(&msg.content.unwrap()).unwrap();
            //这里需要切换到master模式
            // server_ctx.lock().await.
            if register_resp.is_success {
                info!("register success");
            } else {
                error!("register failed,cluster mode must register success. exit !");
                std::process::exit(1);
            }
        }
        Operation::ConvertToMaster => {
            // todo : implement convert to master
            // todo!()
            debug!(
                "receive 'ConvertToMaster' command from registry center:{:?}",
                msg
            );
            convert_self_to_master(server_ctx.clone()).await;
        }
        Operation::ChangeMaster => {
            // todo : implement change master
            // todo!()
            debug!(
                "receive 'ChangeMaster' command from registry center:{:?}",
                msg
            );
            debug!("todo : implement ChangeMaster");
            let new_master_node_info =
                serde_json::from_str::<NodeInfo>(&msg.content.unwrap()).unwrap();
            debug!("new master node info: {:?}", new_master_node_info);
        }
        // 其他不用处理,包括心跳的返回消息
        _ => {}
    }
}
pub async fn convert_self_to_master(
    server_ctx_counter: Arc<Mutex<ServerContext>>,
    // to_registry_center_sender: Sender<ZmqMessage>,
) {
    let mut server_ctx = server_ctx_counter.lock().await;
    // 1.改变自己的node_type.以及节点信息的node_type
    server_ctx.self_node_info.node_type = NodeType::Master;
    server_ctx.node_type = NodeType::Master;

    let to_registry_center_sender = server_ctx.to_registry_center_sender.clone();
    // 2. 关闭slave与原先master的连接,可能可以通过send_error自动关闭,最好还是把那个线程操作权拿出来共享,这里停掉那个线程

    // 3. todo: 开启pub world_update的线程以便其余slave订阅更新 看看do_master这个方法
    // pub_world_sync_thread()

    debug!("todo : implement convert to master");
    to_registry_center_sender
        .send(
            Message::convert_to_master_resp(server_ctx.self_node_info.clone()).to_zmq_dealer_msg(),
        )
        .await
        .unwrap();
    debug!("convert to master success msg have send");
}

/// slave节点从master那里同步世界状态(接收pub的消息),收到消息就同步world user
/// 如果master节点挂了,这里会成为僵尸线程,所以需要能在外部关闭
/// 好像是这里消费的太慢了,导致消息堆积,处理速度太慢,这个接收到的时候就已经很长时间了
pub async fn world_update_from_master_thread(world: Arc<Mutex<World>>, mut sub_socket: SubSocket) {
    debug!("world_update_from_master_thread start");
    loop {
        let recv_msg = sub_socket.recv().await.unwrap();
        tokio::spawn(slave_sync_world_task(world.clone(), recv_msg));
    }
}
pub async fn slave_sync_world_task(world: Arc<Mutex<World>>, recv_msg: ZmqMessage) {
    // let msg = Message::from_zmq_msg(recv_msg);
    //to_string and to world_sync_dto
    // let deserialize_time = time::Instant::now();
    let data_bytes: Vec<u8> = recv_msg.try_into().unwrap();
    let world_sync_dto: WorldSyncDTO = bincode::deserialize(&data_bytes).unwrap();
    // todo: 解码时间太长!!! 35ms 根本没办法满足,必须降到至少20ms以下,换二进制后降到了12ms左右
    // debug!("slave_deserialize_time: {:?}", deserialize_time.elapsed());
    // 同步更新world,单独更新太慢了,开task处理,要不可能recv的消息消费不过来
    // let wait_lock_time = time::Instant::now();
    world.lock().await.sync_from_master(world_sync_dto);
    // debug!("slave_wait_lock_time: {:?}", wait_lock_time.elapsed());
}

/// 这里使用带锁的server_context,因为此方法可能会在不同的线程中多次调用(主从转换,change_master时)
/// todo:考虑把开启forward_msg_thread的调用也放在这里
pub async fn do_slave(server_ctx: &Arc<Mutex<ServerContext>>, world: &Arc<Mutex<World>>) {
    let mut server_ctx = server_ctx.lock().await;
    // todo:
    let pub_addr = server_ctx.master_node.get_pub_addr();
    //1.订阅连接master的pub端口
    let sub_socket = subscribe_to_master_pub(&pub_addr).await;
    //2.单独开一个线程接收master发布的同步信息(这个线程需要world的锁)
    //调用 world_update_from_master_thread()
    tokio::spawn(world_update_from_master_thread(world.clone(), sub_socket));

    //3. 从server_context中获取master_node的main_channel_addr,并且连接
    // 返回main_socket,给slave处理消息使用,todo:这个socket接受到的响应,都需要原封不动的转发给客户端(使用之前handle_msg的sender)
    // todo: 单独开一个线程负责接收master的响应,并且转发给客户端
    let master_node = server_ctx.master_node.clone();
    let slave_to_master_socket = connect_to_master(&master_node.get_key()).await;

    //4. 开启消息转发的线程(与master,client通信交互)
    // todo: to_master_receiver用于将接受到的消息,使用to_master_main_socket转发给master
    let (to_master_sender, to_master_receiver): (Sender<ZmqMessage>, Receiver<ZmqMessage>) =
        tokio::sync::mpsc::channel(100);
    //赋值sender给server_ctx
    server_ctx.to_master_sender = to_master_sender;

    //开启一个线程，用于接收master节点的消息转发给client,并且把client的消息转发给master
    tokio::spawn(forward_master_slave_client_thread(
        slave_to_master_socket,
        server_ctx.to_client_sender.clone(),
        to_master_receiver,
    ));
}
//集群模式下 master节点需要处理
pub async fn do_master(self_node_info: NodeInfo) -> Sender<WorldSyncDTO> {
    //1.开启发布同步世界信息的pubsocket
    let mut pub_socket = PubSocket::new();
    pub_socket
        .bind(&format!(
            "tcp://{}:{}",
            self_node_info.node_addr, self_node_info.pub_port
        ))
        .await
        .expect("Failed to bind pub_scoket");
    let (world_sync_sender, world_sync_receiver): (Sender<WorldSyncDTO>, Receiver<WorldSyncDTO>) =
        tokio::sync::mpsc::channel(200);
    tokio::spawn(pub_world_sync_thread(world_sync_receiver, pub_socket));
    // 返回sender
    world_sync_sender
}

/// only master,pub new world_data
pub async fn pub_world_sync_thread(
    mut world_sync_receiver: Receiver<WorldSyncDTO>,
    mut pub_socket: PubSocket,
) {
    debug!("pub_world_sync_thread start");

    loop {
        let world_sync_dto = world_sync_receiver.recv().await.unwrap();
        //传输之前,更新发送时间,方便接收方计算延迟
        // world_sync_dto.timestamp = Some(chrono::Local::now().timestamp_millis());
        //转为vec<u8>
        let sync_data_bytes = bincode::serialize(&world_sync_dto).unwrap();
        // let msg = Message::world_sync(world_sync_dto);
        // let data_msg = serde_json::to_string(&world_sync_dto).unwrap();
        pub_socket.send(sync_data_bytes.into()).await.unwrap();
    }
}

///连接到服务端 包括main端口和心跳端口
pub async fn connect_to_master(master_main_addr: &str) -> zeromq::DealerSocket {
    let mut options = SocketOptions::default();
    let peer_id_str: String = uuid::Uuid::new_v4().to_string();
    debug!("slave peer_id:{}", peer_id_str);
    options.peer_identity(PeerIdentity::from_str(&peer_id_str).unwrap());

    let mut socket = zeromq::DealerSocket::with_options(options);
    socket
        .connect(&format!("tcp://{}", master_main_addr))
        .await
        .expect("Failed to connect");
    info!("Slave connected to master node");
    socket
}

//slave模式下需要订阅master的pub端口等待同步信息
pub async fn subscribe_to_master_pub(master_pub_addr: &str) -> zeromq::SubSocket {
    let mut socket = SubSocket::new();
    socket
        .connect(&format!("tcp://{}", master_pub_addr))
        .await
        .expect("Failed to connect");
    //订阅所有消息 subscription需要为""
    socket.subscribe("").await.expect("Failed to subscribe");
    info!("Slave connected to master node");
    socket
}

pub async fn do_in_cluster_mode(
    server_ctx_counter: &Arc<Mutex<ServerContext>>,
    config: GameServerConf,
) -> ClusterRequired {
    let mut server_ctx = server_ctx_counter.lock().await;
    let node_type = config.node.node_type.clone();

    //if node_type is slave,need get master_node_addr from registry (http)
    match node_type {
        NodeType::Slave => {
            if let Some(master_node_info_from_registry_center) =
                http_rpc::http_get_master_node().await
            {
                server_ctx.master_node = master_node_info_from_registry_center;
            }
        }
        NodeType::Master => {
            server_ctx.world_sync_sender = do_master(config.node.clone()).await;
        }
    }

    let to_registry_center_socket = connect_to_registry_center_and_register_node(
        config.registry,
        config.node.clone(),
        node_type,
    )
    .await;

    let client_id = config.node.get_key();

    let (to_registry_center_sender, to_registry_center_receiver) =
        tokio::sync::mpsc::channel::<ZmqMessage>(100);
    server_ctx.to_registry_center_sender = to_registry_center_sender;

    // 注册完毕后开启forward线程
    tokio::spawn(forward_server_to_registry_center_thread(
        server_ctx_counter.clone(),
        to_registry_center_socket,
        to_registry_center_receiver,
    ));

    client_id
}
///中转消息(only slave)
/// 1. 接收来自于master_node的响应,转发给客户端
/// 2. 接收来自于客户端的消息,转发给master_node
/// mark: change_master convert_to_master 处理时,此线程可能会被终止,并且可能会重新开启
pub async fn forward_master_slave_client_thread(
    mut slave_to_master_socket: DealerSocket,
    to_client_sender: Sender<ZmqMessage>,
    mut to_master_receiver: Receiver<ZmqMessage>,
) {
    // bug: 那边sender error,报错channel closed,第一条发完是正常的,第二条就error了,是这个线程挂掉了嘛?
    // 哦 忘了加loop,所以收一条就结束了
    loop {
        tokio::select! {
            from_master_resp = slave_to_master_socket.recv() => {

                    // 这里从msg中取出替换的peer_id.重新构建进Message.
                //只有这里接受到的消息需要转换,因为为了确保消息转发给指定的client
                               // todo: 转为自定义msg取出peer_id,替换掉本来的peer_id

                    // let redirect_msg  = redirect_msg.unwrap();

                let msg_str: String  = from_master_resp.unwrap().try_into().unwrap();
                // let bytes = msg.into_vec()[1].clone();
                // let msg_str = String::from_utf8(bytes.to_vec()).unwrap();
                let msg = serde_json::from_str::<Message>(&msg_str).unwrap();
                debug!("Received response from master : {:?}", msg);

                 match msg.client_peer_id.clone(){
                    Some(client_peer_id_str) => {
                        let peer_id = peer_id_str_to_peer_identity(&client_peer_id_str);
                        let rebuild_rep_msg = msg.to_router_msg(peer_id);

                        // debug!("Received response from master : {}", from_master_resp);
                        // let from_master_resp = serde_json::from_str::<common::message::Message>(&from_master_resp).unwrap();
                        //接收到来自master_node的响应,转一下转发给客户端
                        to_client_sender.send(rebuild_rep_msg).await.unwrap();
                    },
                    None => {
                //如果没有peer_id,则不需要转发(slave独自向master发送的操作)

                        debug!("无需转发的消息");
                        continue;
                    },
                }

            }
            msg = to_master_receiver.recv() => {
                match msg {
                    Some(msg_content) => {
                        debug!("Received msg from to_master_sender : {:?},redirect to master", msg_content);

                        match slave_to_master_socket.send(msg_content).await{
                            Ok(_) => {
                                // debug!("send to master success");
                            },
                            Err(e) => {
                                error!("slave send to master error: {},may master is down", e);
                                //todo: 这里是不是就已经可以肯定master挂掉了呢,是否可以在这里结束这个线程
                            }
                        }
                    },
                    None => {
                        // the channel has been closed and there are no more messages
                        error!("to_master_receiver channel closed");

                    },
                }

            }

        }
    }
}
//同步世界信息给其他节点(发布) only master todo: 这里可以使用异步开task去完成
pub async fn sync_world_users_task(maps: Maps, world_sync_sender: Sender<WorldSyncDTO>) {
    // debug!("sync_world_users");
    let mut sync_data = WorldSyncDTO::from_maps(maps);
    //更新时间,用于延迟计算
    sync_data.timestamp = Some(chrono::Local::now().timestamp_millis());
    // let sync_data = serde_json::to_string(&sync_data).unwrap();
    // todo:每过几秒到这里会卡住,大概三秒左右
    // debug!(
    //     "sync_world_users_task user_count: {:?}",
    //     sync_data.user_count()
    // );
    world_sync_sender.send(sync_data.clone()).await.unwrap();
    // 同步的用户数量

    //todo:这里永远拿不到锁,因为调用他的还锁着
    // server_ctx
    //     .lock()
    //     .await
    //     .pub_socket
    //     .send(sync_data.into())
    //     .await
    //     .unwrap();
    // debug!("get lock succes");
}
