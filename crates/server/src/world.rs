use common::basic::{RectScope, User, Vector2, Velocity};
use common::node::StateInfo;
use common_server::map_tool::{self, pos2map_tag, MapTag};
use core::panic;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;
use tracing_subscriber::field::debug;

use crate::config::GameServerConf;
use crate::distribute::WorldSyncDTO;
type UserId = i64;
// impl Ord for MapTag {
//     fn cmp(&self, other: &Self) -> std::cmp::Ordering {
//         self.field_id
//             .cmp(&other.field_id)
//             .then(self.x_len.cmp(&other.x_len))
//             .then(self.y_len.cmp(&other.y_len))
//     }
// }
//单例的世界 server唯一
type Maps = HashMap<MapTag, Map>;
pub struct World {
    // 把整个世界分成多个地图
    pub maps: HashMap<MapTag, Map>, //集群模式下改变的只有这一个数据,其他都是各个server可能有不同的数据
    // 地图里其实已经有一份users,这里是为了方便查找,所以地图user更新时这里要同步更新
    pub users: HashMap<UserId, User>, //slave模式下这个不会更新

    pub width: f32,  //世界的宽度
    pub height: f32, //世界的高度
    //时钟频率(每多少s更新一次世界)
    pub tick_time: f32,
    //current id
    // pub current_id: u64,

    //client_id 存储登录的客户端id以及映射的user_id
    pub client_map: HashMap<String, i64>, //

    pub area_len: u32, //划分的地图区域矩形边长 200000;
    pub max_aoe_radius: f32,
    pub state_info: StateInfo,
}
// map是world中的一块矩形区域,含有一个单独的场景存储当前的实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Map {
    pub map_tag: MapTag,
    pub scenes: Scene,
    pub start_position: Vector2, //地图在world中的起始位置(左上角)
    pub end_position: Vector2,   //地图在world中的结束位置(右下角)
}
impl Map {
    //顶点map中根据顶点坐标获取scope中的user
    pub fn vertex_get_user_by_scope(
        &mut self,
        tag: VertexTag,
        pos: &Vector2,
        end_pos: &Vector2,
    ) -> Vec<User> {
        match tag {
            VertexTag::LeftUp => {
                //print all user count
                println!("all leftup user count:{}", self.scenes.users.len());
                //左上角的end_pos是这个地图的右下顶点位置
                let mut users = Vec::new();
                for user in self.scenes.users.values() {
                    if user.position.x >= pos.x
                        && user.position.y <= pos.y
                        && user.position.x <= end_pos.x
                        && user.position.y >= end_pos.y
                    {
                        users.push(user.clone());
                    }
                }
                users
            }
            VertexTag::LeftDown => {
                println!("all LeftDown user count:{}", self.scenes.users.len());
                //end_pos是map的right_up顶点
                let mut users = Vec::new();
                for user in self.scenes.users.values() {
                    if user.position.x >= pos.x
                        && user.position.y >= pos.y
                        && user.position.x <= end_pos.x
                        && user.position.y <= end_pos.y
                    {
                        users.push(user.clone());
                    }
                }
                users
            }
            VertexTag::RightUp => {
                println!("all RightUp user count:{}", self.scenes.users.len());
                //end_pos是map的left_down顶点
                let mut users = Vec::new();
                for user in self.scenes.users.values() {
                    if user.position.x <= pos.x
                        && user.position.y <= pos.y
                        && user.position.x >= end_pos.x
                        && user.position.y >= end_pos.y
                    {
                        users.push(user.clone());
                    }
                }
                users
            }
            VertexTag::RightDown => {
                println!("all RightDown user count:{}", self.scenes.users.len());
                // end_pos是map的left_up顶点
                let mut users = Vec::new();
                for user in self.scenes.users.values() {
                    if user.position.x <= pos.x
                        && user.position.y >= pos.y
                        && user.position.x >= end_pos.x
                        && user.position.y <= end_pos.y
                    {
                        users.push(user.clone());
                    }
                }
                users
            }
        }
    }
}
///顶点标识
pub enum VertexTag {
    LeftUp,
    LeftDown,
    RightUp,
    RightDown,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    // 场景中的用户 <uid,user实体结构>
    pub users: HashMap<UserId, User>,
}

pub struct ChangeMap {
    pub old_maptag: MapTag,
    pub new_maptag: MapTag,
    pub user: User,
}

//---------impl----------------
impl World {
    //初始化世界指定时钟频率 (单位s)
    pub fn init(conf: GameServerConf) -> World {
        //todo:要把二维世界分为多个区域(最好均分),现在暂时先不做
        // //世界宽度 x:-1000000 - 1000000
        // //世界高度 y:-1000000 - 1000000
        // //世界原点为(0,0)

        // //把整个世界大地图分为10个同等大小的小地图,每个代表一个地图区域
        // //每个地图区域都有一个唯一的id
        // for map_id in 0..9{
        //     //大地图的左上起点为(-1000000,1000000),一次

        // }
        let area_len = 200000;
        // 分区创建世界地图
        // let area_id_list = vec!['A', 'B', 'C', 'D'];
        // let (x_len, y_len) = (1, 1);

        let mut maps = HashMap::new();
        for x_len in -5..6 {
            //fillter 0
            if x_len == 0 {
                continue;
            }
            for y_len in -5..6 {
                if y_len == 0 {
                    continue;
                }
                let map_tag = MapTag {
                    x_len,
                    y_len,
                    field_id: map_tool::get_field_id_from_len_or_pos((x_len as f32, y_len as f32)),
                };
                let map = Map {
                    map_tag,
                    scenes: Scene {
                        users: HashMap::new(),
                    },
                    start_position: Vector2 {
                        x: (map_tag.x_len * area_len) as f32,
                        y: (map_tag.y_len * area_len) as f32,
                    },

                    end_position: Vector2 {
                        x: ((map_tag.x_len + 1) * area_len) as f32,
                        y: ((map_tag.y_len - 1) * area_len) as f32,
                    },
                };
                maps.insert(map_tag, map);
            }
        }

        World {
            maps,
            width: 2000000.0,
            height: 2000000.0,
            tick_time: conf.basic.tick_time,
            client_map: HashMap::new(),
            area_len: area_len as u32,
            users: HashMap::new(),
            max_aoe_radius: conf.basic.max_aoe_radius,
            state_info: StateInfo {
                max_connection: conf.basic.max_connection,
                current_connection: 0,
                load_factor: 0.0,
                avg_update_delay: 0.0,
                user_count: 0,
            },
        }
    }
    // 世界时钟刷新
    pub fn update(&mut self) {
        // println!("world update");
        // 更新世界
        // 更新地图
        // 更新场景

        // 更新当前用户的坐标
        //1. 获取所有的map
        // let mut user_count = 0;

        //更改每一个用户的坐标
        let mut need_change_map_user = vec![];

        for (_map_tag, map) in self.maps.iter_mut() {
            //2. 获取所有的scene
            for (_, user) in map.scenes.users.iter_mut() {
                //3. 更新用户的坐标
                //用户移动前的地图位置
                let old_map_tag = map_tool::pos2map_tag(self.area_len, &user.position);
                user.shift(self.tick_time);

                //4. 判断用户是否需要切换地图(算一下用户移动后应该在哪个地图)
                let new_map_tag = map_tool::pos2map_tag(self.area_len, &user.position);

                if new_map_tag != old_map_tag {
                    //需要切换地图
                    //加入处理的列表
                    need_change_map_user.push(ChangeMap {
                        old_maptag: old_map_tag,
                        new_maptag: new_map_tag,
                        user: user.clone(),
                    });
                }
                // user_count += 1;
            }
        }
        let change_map_count = need_change_map_user.len();
        //切换地图
        for change_map in need_change_map_user {
            //1. 获取旧地图并移除
            let old_map = self.maps.get_mut(&change_map.old_maptag).unwrap();
            old_map.scenes.users.remove(&change_map.user.id);

            //2. 添加到新地图中
            self.maps.get_mut(&change_map.new_maptag).map(|map| {
                map.scenes.users.insert(change_map.user.id, change_map.user);
                map
            });
        }

        // 更新后的数据同步到world的users(是maps的用户集合)
        self.users = self.maps.values().map(|map| map.scenes.users.clone()).fold(
            HashMap::new(),
            |mut acc, x| {
                acc.extend(x);
                acc
            },
        );

        //打印当前世界人数
        // println!("当前世界人数:{}", user_count);
        //打印本次世界更新有几个更换了地图
        // println!("本次世界更新有几个用户更换了地图:{}", change_map_count);

        // todo: 分布式情况下 异步广播进行世界数据同步(users与maps都需要广播给所有可用从节点)
        self.update_state_info();
    }
    //更新state_info,不是用户人数而是连接数(clientmap)
    pub fn update_state_info(&mut self) {
        self.state_info.current_connection = self.client_map.len() as u32;
        self.state_info.load_factor =
            self.state_info.current_connection as f32 / self.state_info.max_connection as f32;
        self.state_info.user_count = self.user_count();
    }
    //世界添加用户(世界更新这里其实应该加锁,出生位置随机)
    pub fn add_user(&mut self, user: User) {
        // 随机生成用户的出生位置,获取不同的maptag加入到不同的地方

        self.maps
            .get_mut(&map_tool::pos2map_tag(self.area_len, &user.position))
            .map(|map| {
                map.scenes.users.insert(user.id, user.clone());
                map
            });
        // 同时更新世界的用户列表
        self.users.insert(user.id, user);

        // self.maps.(MapTag { field_id: 'A', x_len: 1, y_len: 1 }).unwrap().scenes.users.insert(user.id, user);
        // self.maps[0].scenes.users.insert(user.id, user);
    }
    //世界删除(登出)用户
    pub fn remove_user(&mut self, user_id: i64) {
        //1. 获取用户
        match self.users.get(&user_id) {
            Some(user) => {
                // todo: 要使用到user的当前pos信息,所以这个user的pos信息必须是最新的(与map)
                // self.maps[0].scenes.users.remove(&user_id);
                self.maps
                    .get_mut(&map_tool::pos2map_tag(self.area_len, &user.position))
                    .unwrap()
                    .scenes
                    .users
                    .remove(&user_id);
                self.users.remove(&user_id);
            }
            None => {
                debug!("用户本就不存在,无需登出");
            }
        }
    }

    //世界登出用户(by client_id,logout)
    pub fn remove_user_by_client_id(&mut self, client_id: &str) {
        //1. 获取user_id
        let client_map = self.client_map.clone();
        let user_id = client_map.get(client_id);
        //删除client_map中的client_id
        self.client_map.remove(client_id);
        debug!("client_map:{:?}", self.client_map);

        match user_id {
            Some(user_id) => {
                debug!("登出用户id:{}", user_id);
                self.remove_user(*user_id);
            }
            None => {
                debug!("没有对应的user存在,无需登出");
            }
        }
    }
    //用户发起aoe radius< 50m
    pub fn aoe(&mut self, id: i64, radius: f32, money: u64) {
        if radius > self.max_aoe_radius {
            println!("非法aoe范围");

            return;
        }
        //1. 获取用户
        let user = self.users.get(&id);
        if user.is_none() {
            println!("发起aoe的用户不存在,可能掉线了");
            return;
        }
        //打印发起aoe的用户坐标
        println!("发起aoe的用户坐标:{:?}", user.unwrap().position);

        //2.query出这个圆范围内的所有user
        let mut users = self.query_by_circle_scope(user.unwrap().position, radius);
        //3.去除自己(自身不受影响)
        users.remove(&id);
        //4.给所有用户加钱(需要拿用户id去map中找到对应的user)
        println!("aoe影响的用户数量:{}", users.len());
        for user in users.values() {
            //获取用户所在的map
            let map = self
                .maps
                .get_mut(&map_tool::pos2map_tag(self.area_len, &user.position));
            if map.is_none() {
                //bug
                panic!("map is none");
            }
            //获取用户
            let user = map.unwrap().scenes.users.get_mut(&user.id).unwrap();

            //给用户加钱

            user.money += money;
            // println!("给用户:{}加钱: 当前money {}", user.id, user.money);
        }
        //5. 这里不用去同步user数据到world的users(因为我们使用的只有坐标) todo: 考虑是否有问题
    }
    fn query_by_circle_scope(&mut self, circle_center: Vector2, radius: f32) -> HashMap<i64, User> {
        //1.先找出矩形
        //2. 计算出影响范围
        let scope = RectScope {
            left_up: Vector2 {
                x: circle_center.x - radius,
                y: circle_center.y + radius,
            },
            right_down: Vector2 {
                x: circle_center.x + radius,
                y: circle_center.y - radius,
            },
        };
        //3.找出矩形范围内的所有user
        let users = self.query(scope);
        debug!("矩形范围内的所有user数量:{}", users.len());
        //4.计算出圆形范围内的所有user
        let mut users_in_circle = HashMap::new();
        for user in users.iter() {
            //计算出user到圆心的距离
            let distance = (((user.position.x - circle_center.x).powi(2))
                + ((user.position.y - circle_center.y).powi(2)))
            .sqrt()
            .abs();
            //打印用户坐标
            // println!("user:{} 坐标:{:?}", user.id, user.position);
            // println!("user:{} 到圆心的距离:{}", user.id, distance);
            if distance <= radius {
                users_in_circle.insert(user.id, user.clone());
            }
        }
        users_in_circle
    }
    ///
    /// 从世界查询scope范围内的所有user
    pub fn query(&mut self, scope: RectScope) -> Vec<User> {
        let mut users = Vec::new();
        let full_scope = scope.to_full_rect_scope();

        //1. 获取所有相关的map
        let map_tag_vec = self.get_cover_map_by_scope(scope);
        //2. 获取查找范围内的所有user
        for map_tag in map_tag_vec.iter() {
            if let Some(map) = self.maps.get(map_tag) {
                for user in map.scenes.users.values() {
                    if user.position.x >= full_scope.left_up.x
                        && user.position.x <= full_scope.right_down.x
                        && user.position.y <= full_scope.left_up.y
                        && user.position.y >= full_scope.right_down.y
                    {
                        users.push(user.clone());
                    }
                }
            }
        }

        //3. vertex的地图,需要判断user是否在查找范围内(一定是四个相关的)
        // left_up的地图,判断user在查找范围内

        //应该不会有重复的 测试一下
        // println!("查询到的user数量:{}", users.len());
        // //users 整体去重(可能四个顶点都是在同一个小地图内)
        // users.sort_by(|a, b| a.id.cmp(&b.id));
        // users.dedup_by(|a, b| a.id == b.id);
        // println!("去重后的user数量:{}", users.len());

        users
    }

    pub fn print_map(&mut self) {
        //打印地图数量
        println!("地图数量:{}", self.maps.len());
        for (_map_tag, map) in self.maps.iter_mut() {
            println!("map_tag:{:?}", map.map_tag);
            println!("start_position:{:?}", map.start_position);
            println!("end_position:{:?}", map.end_position);
        }
    }
    ///
    /// 给定一个地图的二维坐标,返回这个坐标存在于哪个地图中(map_tag)

    ///给定一个矩形scope(矩形的左上与右下顶点坐标),获取所有相关map的map_tag 顶点相关与中间的分开处理
    /// 为query使用的
    pub fn get_cover_map_by_scope(&self, scope: RectScope) -> Vec<MapTag> {
        //1. 先获取四个顶点相关的map (单独处理)
        //1.1 计算出left_down 与 right_up 两个顶点的坐标
        let left_down = Vector2::new(scope.left_up.x, scope.right_down.y);
        let right_up = Vector2::new(scope.right_down.x, scope.left_up.y);
        // println!("left_down:{:?}", left_down);
        // println!("right_up:{:?}", right_up);
        //1.2 left_up 相关map
        let left_up_map_tag = pos2map_tag(self.area_len, &scope.left_up);
        //1.3 left_down 相关map
        let left_down_map_tag = pos2map_tag(self.area_len, &left_down);
        //1.4 right_up 相关map
        let right_up_map_tag = pos2map_tag(self.area_len, &right_up);
        //1.5 right_down 相关map
        let right_down_map_tag = pos2map_tag(self.area_len, &scope.right_down);

        // //顶点相关的不用去重 后续会单独对user进行去重
        // vertex_map_tag_vec.sort();
        // vertex_map_tag_vec.dedup();

        //2. 获取范围内所有的map (通过各个顶点在的map可以计算出来,这里不包含顶点本身所在的map)
        //包含顶点本身所在的map,所有相关的
        let mut map_tag_vec = Vec::new();
        //
        // println!("left_up_map_tag:{:?}", left_up_map_tag);
        // println!("right_down_map_tag:{:?}", right_down_map_tag);
        let out_start = left_up_map_tag.x_len;
        let out_end = right_down_map_tag.x_len;
        let inside_start = left_up_map_tag.y_len;
        let inside_end = right_down_map_tag.y_len;
        // println!("out_start:{:?}", out_start);
        // println!("out_end:{:?}", out_end);
        // println!("inside_start:{:?}", inside_start);
        // println!("inside_end:{:?}", inside_end);
        let mut x_len = out_start;

        while x_len < out_end + 1 {
            // println!("x_len:{:?}", x_len);
            if x_len == 0 {
                // continue;
                x_len += 1;
            }
            let mut y_len = inside_start;
            while inside_end - 1 < y_len {
                // println!("y_len:{:?}", y_len);
                if y_len == 0 {
                    // continue;
                    y_len -= 1;
                }
                let map_tag = MapTag {
                    field_id: map_tool::get_field_id_from_len_or_pos((x_len as f32, y_len as f32)),
                    x_len,
                    y_len,
                };
                map_tag_vec.push(map_tag);
                y_len -= 1;
            }
            x_len += 1;
        }
        // for(x_len, y_len) in (out_start..out_end).zip(inside_start..inside_end) {
        //     //去掉0
        //     if x_len == 0 || y_len == 0 {
        //         continue;
        //     }
        //     let map_tag = MapTag {
        //         field_id: map_tool::get_field_id_from_len_or_pos((x_len as f32, y_len as f32)),
        //         x_len,
        //         y_len,
        //     };
        //     map_tag_vec.push(map_tag);
        // }
        // for x_len in out_start..out_end {
        //     //去掉0
        //     if x_len == 0 {
        //         continue;
        //     }
        //     for y_len in inside_start..inside_end {
        //         //去掉0
        //         let map_tag = MapTag {
        //             field_id: map_tool::get_field_id_from_len_or_pos((x_len as f32, y_len as f32)),
        //             x_len,
        //             y_len,
        //         };
        //         map_tag_vec.push(map_tag);
        //     }
        // }
        // println!("map_tag_vec:{:?}", map_tag_vec);

        //3. 合并两个结果
        //4. 特殊处理(去重 顶点在区域边界上,顶点在原点上等)

        //5. 返回结果
        map_tag_vec
    }
    pub fn set_velocity(&mut self, id: UserId, velocity: Velocity) {
        if let Some(user) = self.users.get_mut(&id) {
            user.velocity = velocity.clone();
            //同步到map上
            self.maps
                .get_mut(&map_tool::pos2map_tag(self.area_len, &user.position))
                .map(|map| {
                    if let Some(user) = map.scenes.users.get_mut(&id) {
                        user.velocity = velocity;
                    }
                    map
                });
        }
    }
    //----------only distribute slave---------------
    pub fn sync_from_master(&mut self, world_sync_dto: WorldSyncDTO) {
        // debug!("world_sync_dto user_count:{}", world_sync_dto.user_count());
        //1. 同步map中的user
        for (maptag, user_data_in_map) in world_sync_dto.data {
            //1.1 获取map
            let map = self.maps.get_mut(&MapTag::from_string(&maptag));
            if let Some(map) = map {
                //1.2 更新场景中的user
                map.scenes.users = user_data_in_map;
            }
        }
        self.update_state_info();
        // debug!(
        //     "world sync_from_master,current user count:{},sync cost time:{:?} ms",
        //     self.user_count(),
        //     chrono::Local::now().timestamp_millis() - world_sync_dto.timestamp.unwrap()
        // );
        // slave的world不需要同步users到world中,登出之类的操作也会转发到master
    }
    pub fn get_world_sync_dto(&self) -> WorldSyncDTO {
        let mut data = HashMap::new();
        for (maptag, map) in &self.maps {
            data.insert(maptag.clone().to_string(), map.scenes.users.clone());
        }
        WorldSyncDTO {
            data,
            timestamp: None,
        }
    }
    pub fn user_count(&self) -> u32 {
        //计算当前地图中所有user的数量
        let mut count = 0;
        for map in self.maps.values() {
            count += map.scenes.users.len();
        }
        count as u32
    }
}
#[cfg(False)]
#[cfg(test)]
mod world_tests {
    use common_server::map_tool::{self, MapTag};

    use crate::server_run::read_config;

    #[test]
    fn test_pos2map_tag() {
        let conf = read_config();

        let mut world = super::World::init(conf);

        // 原点
        let pos = super::Vector2 { x: 0.0, y: 0.0 };
        let map_tag = map_tool::pos2map_tag(world.area_len, &pos);
        assert_eq!(
            map_tag,
            MapTag {
                field_id: 'A',
                x_len: 1,
                y_len: 1
            }
        );
        // println!("map_tag:{:?}", map_tag);

        // x轴正方向
        let pos = super::Vector2 { x: 1.0, y: 0.0 };
        let map_tag = map_tool::pos2map_tag(world.area_len, &pos);

        assert_eq!(
            map_tag,
            MapTag {
                field_id: 'B',
                x_len: 1,
                y_len: 1
            }
        );

        // x轴负方向
        let pos = super::Vector2 { x: -1.0, y: 0.0 };
        let map_tag = map_tool::pos2map_tag(world.area_len, &pos);

        assert_eq!(
            map_tag,
            MapTag {
                field_id: 'C',
                x_len: 1,
                y_len: 1
            }
        );

        // y轴正方向
        let pos = super::Vector2 { x: 0.0, y: 1.0 };
        let map_tag = map_tool::pos2map_tag(world.area_len, &pos);

        assert_eq!(
            map_tag,
            MapTag {
                field_id: 'A',
                x_len: 1,
                y_len: 1
            }
        );

        // y轴负方向
        let pos = super::Vector2 { x: 0.0, y: -1.0 };
        let map_tag = map_tool::pos2map_tag(world.area_len, &pos);

        assert_eq!(
            map_tag,
            MapTag {
                field_id: 'D',
                x_len: 1,
                y_len: 1
            }
        );
    }

    #[test]
    fn test_get_cover_map_by_scope() {
        let conf = read_config();

        let world = super::World::init(conf);

        let scope = super::RectScope {
            left_up: super::Vector2 {
                x: -790010.0,
                y: 400000.0,
            },
            right_down: super::Vector2 {
                x: -300000.0,
                y: 190000.0,
            },
        };
        let map_tag_vec = world.get_cover_map_by_scope(scope);
        println!("map_tag_vec:{:?}", map_tag_vec);
    }
    #[test]
    fn test_full_scope_get_cover_map() {
        let conf = read_config();

        let world = super::World::init(conf);
        //全图
        let scope = super::RectScope {
            left_up: super::Vector2 {
                x: -1000000.0,
                y: 1000000.0,
            },
            right_down: super::Vector2 {
                x: 1000000.0,
                y: -1000000.0,
            },
        };
        let inside_map_tag_vec = world.get_cover_map_by_scope(scope);
        println!("map_tag_vec.len:{:?}", inside_map_tag_vec.len());
    }
}
#[cfg(False)]
#[cfg(test)]
mod map_tool_tests {
    use common_server::map_tool::{self, MapTag};

    use crate::{server_run::read_config, world::World};

    #[test]
    fn test_pos2map_tag_len() {
        let conf = read_config();

        let mut world = World::init(conf);

        let pos = super::Vector2 { x: 0.0, y: 0.0 };
        let map_tag = map_tool::pos2map_tag(world.area_len, &pos);

        assert_eq!(
            map_tag,
            MapTag {
                field_id: 'A',
                x_len: 1,
                y_len: 1
            }
        );

        let pos = super::Vector2 {
            x: 400000.0,
            y: 400000.0,
        };
        let map_tag = map_tool::pos2map_tag(world.area_len, &pos);

        assert_eq!(
            map_tag,
            MapTag {
                field_id: 'B',
                x_len: 2,
                y_len: 2
            }
        );

        let pos = super::Vector2 {
            x: -400000.0,
            y: 400000.0,
        };
        let map_tag = map_tool::pos2map_tag(world.area_len, &pos);

        assert_eq!(
            map_tag,
            MapTag {
                field_id: 'A',
                x_len: -2,
                y_len: 2
            }
        );

        let pos = super::Vector2 {
            x: -400001.0,
            y: 400000.0,
        };
        let map_tag = map_tool::pos2map_tag(world.area_len, &pos);

        assert_eq!(
            map_tag,
            MapTag {
                field_id: 'A',
                x_len: -3,
                y_len: 2
            }
        );
    }
}
