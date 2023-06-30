mod http_req;
use std::str::FromStr;

use common::{basic::User, message::Message};
use http_req::get_server_connection;
use tokio::time::sleep;
use zeromq::{
    util::PeerIdentity, DealerSocket, Socket, SocketOptions, SocketRecv, SocketSend, ZmqMessage,
};
type UserId = i64;
#[tokio::main]
async fn main() {
    //启动时生成client_id
    let client_id = uuid::Uuid::new_v4().to_string();

    // 需要搞个hb客户端处理 服务端断联 自动退出或是重连其他服务器

    let mut socket = connect_to_server(&client_id).await;

    for _ in 0..10 {
        let login_msg = serde_json::to_string(&Message::login(&client_id)).unwrap();

        let mut zmq_msg: ZmqMessage = login_msg.into();
        //消息的第一部分加上一个空的frame
        zmq_msg.push_front("".into());

        socket.send(zmq_msg).await.unwrap();
        match socket.recv().await {
            Ok(msg) => println!("Received: {:?}", msg),
            Err(e) => println!("Error: {:?}", e),
        }
    }
}
pub async fn heartbeat_thread(client_id: String, heartbeat_addr: String) {
    // println!("hb client_id {}", client_id);
    //先连接上服务端的心跳端口
    let mut heartbeat_socket = zeromq::ReqSocket::new();
    heartbeat_socket
        .connect(&heartbeat_addr)
        .await
        .expect("Failed to connect");
    loop {
        let ping_start = std::time::Instant::now();
        let ping_msg = serde_json::to_string(&Message::ping(&client_id)).unwrap();

        heartbeat_socket.send(ping_msg.into()).await.unwrap();

        match heartbeat_socket.recv().await {
            Ok(_msg) => {
                let _pong = ping_start.elapsed().as_millis();
                // println!("ping: {:?}", ping);
            }
            Err(e) => println!("Error: {:?}", e),
        }
        sleep(std::time::Duration::from_secs(1)).await;
    }
}
///连接到服务端 包括main端口和心跳端口
pub async fn connect_to_server(client_id: &str) -> zeromq::DealerSocket {
    tokio::spawn(heartbeat_thread(
        client_id.to_string(),
        String::from("tcp://127.0.0.1:5556"),
    ));
    let mut options = SocketOptions::default();
    let peer_id_str: String = uuid::Uuid::new_v4().to_string();
    options.peer_identity(PeerIdentity::from_str(&peer_id_str).unwrap());

    let mut socket = zeromq::DealerSocket::with_options(options);
    socket
        .connect("tcp://127.0.0.1:5555")
        .await
        .expect("Failed to connect");
    println!("Connected to server");
    socket
}

//分布式版本的connect_to_server
pub async fn connect_to_server_distributed(client_id: &str) -> zeromq::DealerSocket {
    //发起http请求从注册中心获取可用的连接地址
    let node_info = get_server_connection().await;
    if node_info.is_none() {
        panic!("no available server");
    }
    let node_info = node_info.unwrap();
    let heart_beat_addr = format!("tcp://{}:{}", node_info.node_addr, node_info.heartbeat_port);
    let main_addr = format!(
        "tcp://{}:{}",
        node_info.node_addr, node_info.main_channel_port
    );
    tokio::spawn(heartbeat_thread(client_id.to_string(), heart_beat_addr));
    let mut options = SocketOptions::default();
    let peer_id_str: String = uuid::Uuid::new_v4().to_string();
    println!("client peer_id is {}", peer_id_str);
    options.peer_identity(PeerIdentity::from_str(&peer_id_str).unwrap());

    let mut socket = zeromq::DealerSocket::with_options(options);
    socket.connect(&main_addr).await.expect("Failed to connect");
    println!("Connected to server");
    socket
}

#[inline]
async fn full_dealer_msg(msg: &str) -> zeromq::ZmqMessage {
    let mut zmq_msg: ZmqMessage = msg.into();
    //消息的第一部分加上一个空的frame
    zmq_msg.push_front("".into());
    zmq_msg
}
async fn login_distributed() -> (String, UserId, zeromq::DealerSocket) {
    let client_id = uuid::Uuid::new_v4().to_string();
    let socket = connect_to_server_distributed(&client_id).await;
    _login(&client_id, socket).await
}
async fn login() -> (String, UserId, zeromq::DealerSocket) {
    let client_id = uuid::Uuid::new_v4().to_string();
    let socket = connect_to_server(&client_id).await;
    _login(&client_id, socket).await
}
async fn _login(
    client_id: &str,
    mut socket: DealerSocket,
) -> (String, UserId, zeromq::DealerSocket) {
    let login_msg: ZmqMessage = Message::login(&client_id).to_zmq_dealer_msg();
    socket.send(login_msg).await.unwrap();
    //响应就是user_id
    let user_id = match socket.recv().await {
        Ok(msg) => {
            // println!("msg:{:?}", msg);
            let msg_str: String = msg.try_into().unwrap();
            // let bytes = msg.into_vec()[1].clone();
            // let msg_str = String::from_utf8(bytes.to_vec()).unwrap();
            let msg = serde_json::from_str::<Message>(&msg_str).unwrap();

            // println!("msg:{:?}", msg);
            let user_id = msg.content.unwrap().parse::<UserId>().unwrap();
            println!("login success, user_id: {:?}", user_id);

            // println!("Received: {:?}", msg);
            // user_id.parse::<u64>().unwrap()
            // 1001
            user_id
        }
        Err(e) => {
            panic!("Error: {:?}", e)
        }
    };
    (client_id.to_string(), user_id, socket)
}
pub async fn recv_msg(socket: &mut zeromq::DealerSocket, count: usize, opt: &str,start_time:tokio::time::Instant) {
    for _ in 0..count {
        match socket.recv().await {
            Ok(msg) => {
                println!("{} cost time: {:?}",opt, start_time.elapsed());
                // println!("{} Received:  ", opt);
                if opt == "query_full"{
                    //query响应太大了，不打印全部,打印个数吧
                    let user_str:String = msg.try_into().unwrap();
                    let my_msg = serde_json::from_str::<Message>(&user_str).unwrap();
                    let users: Vec<User> = serde_json::from_str(&my_msg.content.unwrap()).unwrap();
                    println!("user count: {}",users.len());
                    continue;
                }
                println!("msg:{:?}", msg);
            }
            Err(e) => println!("Error: {:?}", e),
        }
    }
}
#[cfg(False)]
mod query_test {
    use common::{
        basic::{RectScope, Vector2},
        message::Message,
    };
    use zeromq::SocketSend;

    // use crate::login_distributed;
    use crate::{login_distributed, recv_msg};
    // #[tokio::test]
    async fn query_scope() {
        let client_id = uuid::Uuid::new_v4().to_string();
        let mut socket = super::connect_to_server(&client_id).await;

        let scope = RectScope {
            left_up: Vector2 {
                x: -790010.0,
                y: 400000.0,
            },
            right_down: Vector2 {
                x: -300000.0,
                y: 190000.0,
            },
        };
        // let login_msg = serde_json::to_string(&super::Message::login(&client_id)).unwrap();
        let query_msg = serde_json::to_string(&super::Message::query(&client_id, scope)).unwrap();
        // let zmq_login_msg = super::full_dealer_msg(&login_msg).await;

        let zmq_query_msg = super::full_dealer_msg(&query_msg).await;
        // socket.send(zmq_login_msg.clone()).await.unwrap();
        // sleep(std::time::Duration::from_secs(1)).await;

        socket.send(zmq_query_msg).await.unwrap();
        // sleep(std::time::Duration::from_secs(1)).await;
        recv_msg(&mut socket, 1, "query_scope").await;
    }
}
#[cfg(False)]
mod ditribute_tests {
    use common::{
        basic::{RectScope, Vector2, Velocity},
        message::Message,
    };
    use tokio::time::sleep;
    use zeromq::SocketSend;

    use crate::{login_distributed, recv_msg};

    #[tokio::test]
    async fn login() {
        // let mut socket = super::connect_to_server(&client_id).await;
        let (client_id, _, mut socket) = login_distributed().await;

        let login_msg = serde_json::to_string(&super::Message::login(&client_id)).unwrap();
        let zmq_msg = super::full_dealer_msg(&login_msg).await;
        //已经登录的报错
        socket.send(zmq_msg).await.unwrap();
        recv_msg(&mut socket, 1, "test_login").await;
        // 测试user_count有没有增加,退出的话心跳停止会自动下线
        sleep(std::time::Duration::from_secs(10)).await;
    }
    #[tokio::test]
    async fn logout() {
        let (client_id, _, mut socket) = login_distributed().await;
        //这里登录成功了,等待一下再注销
        sleep(std::time::Duration::from_secs(10)).await;

        let zmq_msg = Message::logout(&client_id).to_zmq_dealer_msg();
        socket.send(zmq_msg).await.unwrap();
        recv_msg(&mut socket, 1, "test_logout").await;
    }
    //todo:把其他几个测试也都弄上去
    #[tokio::test]
    async fn query_full_world() {
        let (client_id, _user_id, mut socket) = login_distributed().await;

        //全图
        let scope: RectScope = RectScope {
            left_up: Vector2 {
                x: -1000000.0,
                y: 1000000.0,
            },
            right_down: Vector2 {
                x: 1000000.0,
                y: -1000000.0,
            },
        };
        let zmq_query_msg = Message::query(&client_id, scope).to_zmq_dealer_msg();

        socket.send(zmq_query_msg).await.unwrap();
        recv_msg(&mut socket, 1, "query_full").await;
    }
    #[tokio::test]
    async fn aoe() {
        let (client_id, user_id, mut socket) = login_distributed().await;

        let zmq_aoe_msg = Message::aoe(&client_id, user_id, 20.0, 100).to_zmq_dealer_msg();

        socket.send(zmq_aoe_msg).await.unwrap();

        recv_msg(&mut socket, 1, "test_aoe").await;
    }
    #[tokio::test]
    async fn set_velocity() {
        let (client_id, user_id, mut socket) = login_distributed().await;

        let zmq_set_velocity_msg: zeromq::ZmqMessage = Message::set_velocity(
            &client_id,
            user_id,
            Velocity {
                direction: Vector2::right(),
                len: 20.0,
            },
        )
        .to_zmq_dealer_msg();

        socket.send(zmq_set_velocity_msg).await.unwrap();
        recv_msg(&mut socket, 1, "test_velocity").await;
    }
}

#[cfg(False)]
mod client_tests {
    //todo: 需要登录回复的并发请求,4个有两个都接收不到,server报错
    // (可能是因为test的原因,多个test,recv还会接收到别的test的返回,他们可能使用的是同一个peerid,客户端是多个进程的情况下即使并发应该不会出现)
    use common::basic::{RectScope, Vector2, Velocity};
    use tokio::time::sleep;
    use zeromq::SocketSend;

    use crate::{login, recv_msg};

    #[tokio::test]
    async fn test_login() {
        // let mut socket = super::connect_to_server(&client_id).await;
        let (client_id, _, mut socket) = login().await;

        let login_msg = serde_json::to_string(&super::Message::login(&client_id)).unwrap();
        let zmq_msg = super::full_dealer_msg(&login_msg).await;
        //已经登录的报错
        socket.send(zmq_msg).await.unwrap();
        recv_msg(&mut socket, 1, "test_login").await;
    }
    #[tokio::test]
    async fn test_logout() {
        let (client_id, _, mut socket) = login().await;

        let logout_msg = serde_json::to_string(&super::Message::logout(&client_id)).unwrap();

        let zmq_logout_msg = super::full_dealer_msg(&logout_msg).await;

        socket.send(zmq_logout_msg).await.unwrap();

        recv_msg(&mut socket, 1, "test_logout").await;
    }

    #[tokio::test]
    async fn aoe() {
        let (client_id, user_id, mut socket) = login().await;

        let aoe_msg =
            serde_json::to_string(&super::Message::aoe(&client_id, user_id, 190000.0, 100))
                .unwrap();
        let zmq_aoe_msg = super::full_dealer_msg(&aoe_msg).await;

        socket.send(zmq_aoe_msg).await.unwrap();

        recv_msg(&mut socket, 1, "test_aoe").await;
    }
    #[tokio::test]
    async fn set_velocity() {
        let (client_id, user_id, mut socket) = login().await;

        let set_velocity_msg = serde_json::to_string(&super::Message::set_velocity(
            &client_id,
            user_id,
            Velocity {
                direction: Vector2::right(),
                len: 20.0,
            },
        ))
        .unwrap();

        let zmq_set_velocity_msg = super::full_dealer_msg(&set_velocity_msg).await;
        socket.send(zmq_set_velocity_msg).await.unwrap();
        recv_msg(&mut socket, 1, "test_velocity").await;
    }
    #[tokio::test]
    async fn query_full_world() {
        let (client_id, _user_id, mut socket) = login().await;

        //全图
        let scope: RectScope = RectScope {
            left_up: Vector2 {
                x: -1000000.0,
                y: 1000000.0,
            },
            right_down: Vector2 {
                x: 1000000.0,
                y: -1000000.0,
            },
        };
        let query_msg = serde_json::to_string(&super::Message::query(&client_id, scope)).unwrap();

        let zmq_query_msg = super::full_dealer_msg(&query_msg).await;

        socket.send(zmq_query_msg).await.unwrap();
        recv_msg(&mut socket, 1, "query_full").await;
    }
    #[tokio::test]
    async fn query_scope() {
        let client_id = uuid::Uuid::new_v4().to_string();
        let mut socket = super::connect_to_server(&client_id).await;

        let scope = RectScope {
            left_up: Vector2 {
                x: -790010.0,
                y: 400000.0,
            },
            right_down: Vector2 {
                x: -300000.0,
                y: 190000.0,
            },
        };
        // let login_msg = serde_json::to_string(&super::Message::login(&client_id)).unwrap();
        let query_msg = serde_json::to_string(&super::Message::query(&client_id, scope)).unwrap();
        // let zmq_login_msg = super::full_dealer_msg(&login_msg).await;

        let zmq_query_msg = super::full_dealer_msg(&query_msg).await;
        // socket.send(zmq_login_msg.clone()).await.unwrap();
        // sleep(std::time::Duration::from_secs(1)).await;

        socket.send(zmq_query_msg).await.unwrap();
        // sleep(std::time::Duration::from_secs(1)).await;
        recv_msg(&mut socket, 1, "query_scope").await;
    }
}
#[cfg(test)]
mod single_pressure_test {
    use common::{
        basic::{RectScope, Vector2, Velocity},
        message::Message,
    };
    use zeromq::SocketSend;

    use crate::{connect_to_server, login, recv_msg};
    use tokio::time::{sleep, self};

    fn get_reqeust_count() -> u32 {
        100
    }
    #[tokio::test]
    async fn test_login() {
        // let mut socket = super::connect_to_server(&client_id).await;
        let (client_id, _, mut socket) = login().await;

        let login_msg = serde_json::to_string(&super::Message::login(&client_id)).unwrap();
        let zmq_msg = super::full_dealer_msg(&login_msg).await;
        //已经登录的报错
        socket.send(zmq_msg).await.unwrap();
        recv_msg(&mut socket, 1, "test_login",time::Instant::now()).await;
        // 测试user_count有没有增加,退出的话心跳停止会自动下线
        sleep(std::time::Duration::from_secs(30)).await;
    }
    #[tokio::test]
    async fn test_logout() {
        let (client_id, _, mut socket) = login().await;
        //这里登录成功了,等待一下再注销
        sleep(std::time::Duration::from_secs(30)).await;

        let zmq_msg = Message::logout(&client_id).to_zmq_dealer_msg();
        socket.send(zmq_msg).await.unwrap();
        recv_msg(&mut socket, 1, "test_logout",time::Instant::now()).await;
    }
    //查询只需连接 无需登录
    #[tokio::test]
    async fn query_full_world() {
        // let (client_id, _user_id, mut socket) = login().await;
        let client_id = uuid::Uuid::new_v4().to_string();

        let mut socket = connect_to_server(&client_id).await;
        //全图
        let scope: RectScope = RectScope {
            left_up: Vector2 {
                x: -1000000.0,
                y: 1000000.0,
            },
            right_down: Vector2 {
                x: 1000000.0,
                y: -1000000.0,
            },
        };
        let query_msg = Message::query(&client_id, scope).to_zmq_dealer_msg();

        for _ in 0..get_reqeust_count() {
            socket.send(query_msg.clone()).await.unwrap();
            recv_msg(&mut socket, 1, "query_full",time::Instant::now()).await;
            sleep(std::time::Duration::from_secs_f32(0.4)).await;
        }
    }
    #[tokio::test]
    async fn aoe() {
        let (client_id, user_id, mut socket) = login().await;

        let zmq_aoe_msg = Message::aoe(&client_id, user_id, 20.0, 100).to_zmq_dealer_msg();

        //连发100个请求
        for _ in 0..get_reqeust_count() {
            socket.send(zmq_aoe_msg.clone()).await.unwrap();
            recv_msg(&mut socket, 1, "test_aoe",time::Instant::now()).await;
            sleep(std::time::Duration::from_secs_f32(0.4)).await;
        }
    }
    #[tokio::test]
    async fn set_velocity() {
        let (client_id, user_id, mut socket) = login().await;

        let zmq_set_velocity_msg: zeromq::ZmqMessage = Message::set_velocity(
            &client_id,
            user_id,
            Velocity {
                direction: Vector2::right(),
                len: 20.0,
            },
        )
        .to_zmq_dealer_msg();
        //连发100个请求
        for _ in 0..get_reqeust_count() {
            socket.send(zmq_set_velocity_msg.clone()).await.unwrap();
            recv_msg(&mut socket, 1, "test_velocity",time::Instant::now()).await;
            sleep(std::time::Duration::from_secs_f32(0.4)).await;
        }
    }
}
