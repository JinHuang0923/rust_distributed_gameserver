use std::str::FromStr;

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::json;
use zeromq::{util::PeerIdentity, ZmqMessage};

use crate::{
    basic::{RectScope, User, Velocity},
    node::{NodeInfo, NodeType, StateInfo},
};
type UserId = i64;

#[derive(Debug, Clone, Deserialize, Serialize)]
//使用蛇形命名法
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Ping,
    Pong,
    Login,
    Logout,
    Aoe,
    SetVelocity,
    Query,
    //reply
    Reply,
    QueryResp,

    // for distribute
    Register,
    Report, //report state
    //----only master node-----
    AddUser, //与client无关,直接由server之间转发使用,添加用户到master世界
    DelUser, //与client无关,直接由server之间转发使用,删除用户从master世界
    //----only registry center(注册中心发起的指令)-----
    ConvertToMaster, //接收到消息的node转换为master_node
    ChangeMaster, //接收到消息的slave_node需要更换master_node,包括重新连接转发channel,重新订阅新的master信息
    RegisterResp, //注册中心给的注册节点的响应
}
type PeerIdStr = String;
pub fn peer_identity_to_string(peer_id: PeerIdentity) -> PeerIdStr {
    let peer_id_bytes: Bytes = peer_id.into();
    let peer_id_str = String::from_utf8(peer_id_bytes.into()).unwrap();

    peer_id_str
}
pub fn peer_id_str_to_peer_identity(peer_id_str: &PeerIdStr) -> PeerIdentity {
    // let peer_id = peer_id_str.as_bytes().to_vec();
    let peer_id_str = peer_id_str.clone();
    PeerIdentity::from_str(&peer_id_str).unwrap()
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Message {
    // 用于区别客户端
    pub client_id: String,
    // 登录之后会给client分配一个user_id,用于区别user以及后续的操作,更方便server之间转发处理,类似于cookie的唯一标识
    pub user_id: Option<UserId>,
    // 操作 用于判断客户端发起的是什么类型的交互
    pub operation: Operation,
    // 操作相关的内容(可能无内容,包起来)
    pub content: Option<String>,
    pub client_peer_id: Option<PeerIdStr>, //peeridentity转换为string,用于接收master响应,转发到指定客户端使用
}
impl Message {
    //tool
    pub fn to_json_str(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
    // slave接收到消息后,会把消息的peer_id设置上,用于后面接收到master响应后转发到对应客户端
    pub fn set_peer_id(&mut self, peer_id: &PeerIdStr) -> Self {
        self.client_peer_id = Some(peer_id.to_string());
        self.clone()
    }
    pub fn set_peer_id_opt(&mut self, peer_id_opt: Option<String>) -> Self {
        self.client_peer_id = peer_id_opt;
        self.clone()
    }
    //-----以下几类都是基于zmq不同socket的消息格式转换-----
    pub fn to_zmq_dealer_msg(&self) -> ZmqMessage {
        let mut msg: ZmqMessage = serde_json::to_string(self).unwrap().into();
        //加一个空帧, 用于分隔
        msg.push_front("".into());
        msg
    }
    pub fn to_router_msg(&self, peer_id: PeerIdentity) -> ZmqMessage {
        let mut msg: ZmqMessage = serde_json::to_string(self).unwrap().into();
        // 最前方加上peer_id
        msg.push_front(peer_id.into());
        msg
    }
    //普通的REPSOCKET响应时使用的消息格式,不需要peer_id,empty frame etc..
    pub fn to_rep_msg(&self) -> ZmqMessage {
        let msg: ZmqMessage = serde_json::to_string(self).unwrap().into();
        msg
    }
    /// 第0帧就是正文的msg可以这样解析
    pub fn from_zmq_reply_msg(zmq_msg: ZmqMessage) -> Self {
        //先转为string
        let msg_str: String = zmq_msg.try_into().unwrap();
        //转为Message
        let msg: Message = serde_json::from_str(&msg_str).unwrap();
        msg
    }

    //常用的一些客户端/服务端消息操作
    //ping
    pub fn ping(client_id: &str) -> Message {
        Message {
            client_id: client_id.to_string(),
            user_id: None,
            operation: Operation::Ping,
            content: None,
            client_peer_id: None,
        }
    }

    //login
    pub fn login(client_id: &str) -> Message {
        Message {
            client_id: client_id.to_string(),
            user_id: None,
            operation: Operation::Login,
            content: None,
            client_peer_id: None,
        }
    }
    //logout
    pub fn logout(client_id: &str) -> Message {
        Message {
            client_id: client_id.to_string(),
            user_id: None,
            operation: Operation::Logout,
            content: None,
            client_peer_id: None,
        }
    }

    //aoe
    pub fn aoe(client_id: &str, user_id: UserId, radius: f32, money: u64) -> Message {
        Message {
            client_id: client_id.to_string(),
            user_id: Some(user_id),
            operation: Operation::Aoe,
            content: Some(format!("{} {}", radius, money)),
            client_peer_id: None,
        }
    }
    //set_velocity
    pub fn set_velocity(client_id: &str, user_id: UserId, velocity: Velocity) -> Message {
        Message {
            client_id: client_id.to_string(),
            user_id: Some(user_id),
            operation: Operation::SetVelocity,
            content: Some(serde_json::to_string(&velocity).unwrap()),
            client_peer_id: None,
        }
    }
    //query
    pub fn query(client_id: &str, scope: RectScope) -> Message {
        Message {
            client_id: client_id.to_string(),
            user_id: None,
            operation: Operation::Query,
            content: Some(serde_json::to_string(&scope).unwrap()),
            client_peer_id: None,
        }
    }
    //-----------------abount distribute------------------
    pub fn register(node_info: NodeInfo, node_type: NodeType) -> Self {
        //client_id = node_addr:main_channel_port
        // let addr = format!("{}:{}", node_addr, main_channel_port);
        // let content_json = json!({
        //     "node_addr": node_addr,
        //     "main_channel_port": main_channel_port,
        //     "heartbeat_port": heartbeat_port,
        //     "node_type": node_type,
        // });
        Message {
            client_id: node_info.get_key(),
            operation: Operation::Register,
            content: Some(serde_json::to_string(&node_info).unwrap()),
            user_id: None,
            client_peer_id: None,
        }
    }
    pub fn state_report(client_id: &str, state_info: StateInfo) -> Message {
        Message {
            client_id: client_id.to_string(),
            user_id: None,
            operation: Operation::Report,
            content: Some(serde_json::to_string(&state_info).unwrap()),
            client_peer_id: None,
        }
    }
    pub fn add_user(client_id: &str, user: User) -> Self {
        Message {
            client_id: client_id.to_string(),
            user_id: Some(user.id),
            operation: Operation::AddUser,
            content: Some(serde_json::to_string(&user).unwrap()),
            client_peer_id: None,
        }
    }
    pub fn del_user(client_id: &str, user_id: UserId) -> Self {
        Message {
            client_id: client_id.to_string(),
            user_id: Some(user_id),
            operation: Operation::DelUser,
            content: None,
            client_peer_id: None,
        }
    }
    //---------registry center commond----------------
    pub fn convert_to_master(client_id: &str) -> Self {
        //response的client_id是请求的client_id,只是顺便带上可能客户端需要验证是否是自己的消息
        Message {
            client_id: client_id.to_string(),
            user_id: None,
            operation: Operation::ConvertToMaster,
            content: None,
            client_peer_id: None,
        }
    }
    pub fn change_master(client_id: &str, master_node_info: NodeInfo) -> Self {
        Message {
            client_id: client_id.to_string(),
            user_id: None,
            operation: Operation::ChangeMaster,
            content: Some(serde_json::to_string(&master_node_info).unwrap()),
            client_peer_id: None,
        }
    }
    //-----------------resp--------------
    //pong
    pub fn pong(client_id: &str) -> Message {
        Message {
            client_id: client_id.to_string(),
            user_id: None,
            operation: Operation::Pong,
            content: None,
            client_peer_id: None,
        }
    }
    pub fn resp(content: &str) -> Self {
        Message {
            client_id: "".to_string(),
            user_id: None,
            operation: Operation::Reply,
            content: Some(content.to_string()),
            client_peer_id: None,
        }
    }
    pub fn query_resp(users: Vec<User>) -> Self {
        Message {
            client_id: "".to_string(),
            user_id: None,
            operation: Operation::QueryResp,
            content: Some(serde_json::to_string(&users).unwrap()),
            client_peer_id: None,
        }
    }
    pub fn register_resp(resp: RegisterResp) -> Self {
        Message {
            client_id: "".to_string(),
            user_id: None,
            operation: Operation::RegisterResp,
            content: Some(serde_json::to_string(&resp).unwrap()),
            client_peer_id: None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterResp {
    pub is_success: bool,
    pub reason: Option<String>,
}
impl RegisterResp {
    pub fn new(is_success: bool, reason: Option<String>) -> Self {
        Self { is_success, reason }
    }
    pub fn success() -> Self {
        Self::new(true, None)
    }
    pub fn fail(reason: &str) -> Self {
        Self::new(false, Some(reason.to_string()))
    }
}
// todo: 写一个response的结构体, 用于返回给客户端的数据（用protobuf实现）
