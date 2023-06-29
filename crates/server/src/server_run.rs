use std::{
    collections::{HashMap, VecDeque},
    error,
    fs::File,
    io::Read,
    sync::Arc,
};

use common::{
    basic::User,
    message::Message,
    node::{NodeType, StateInfo},
};
use heartbeat::heartbeat_update;
use idgenerator::{IdGeneratorOptions, IdInstance};
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        Mutex,
    },
    task::JoinHandle,
    time::{self, sleep},
};

use crate::{
    config::{GameServerConf, Mode, Registry},
    distribute::{do_in_cluster_mode, do_slave, sync_world_users_task, try_report_load_factor_task},
    heartbeat::heartbeat_monitor,
    http_rpc,
    main_handle::{add_user2_world, main_channel_thread, UserType},
    server_context::{self, ServerContext},
    world::World,
};
use tracing::{debug, error};
use zeromq::{DealerSocket, Socket, SocketSend, ZmqMessage};
pub fn read_config(config_file_path: &str) -> GameServerConf {
    // path = 当前项目下的config/config.toml
    // let mut current_dir = std::env::current_dir().unwrap();
    // // println!("current_dir: {:?}", current_dir);
    // current_dir.push("config/conf.toml");
    // let config_file_path = current_dir.to_str().unwrap();
    let mut file = match File::open(config_file_path) {
        Ok(f) => f,
        Err(e) => panic!("no such file {} exception:{}", config_file_path, e),
    };
    let mut str_val = String::new();
    match file.read_to_string(&mut str_val) {
        Ok(s) => s,
        Err(e) => panic!("Error Reading file: {}", e),
    };
    let config: GameServerConf = toml::from_str(&str_val).unwrap();
    // dbg!(config.clone());
    config
}

pub async fn run(conf_path: String) -> (Arc<Mutex<World>>, JoinHandle<()>) {
    let config = read_config(&conf_path);

    // Setup the option for the id generator instance.
    let options = IdGeneratorOptions::new().worker_id(1).worker_id_bit_len(6);
    // Initialize the id generator instance with the option.
    // Other options not set will be given the default value.
    IdInstance::init(options).unwrap();

    let mut world = World::init(config.clone());
    //世界初始化
    //创建两个随机用户
    let first_user = User::random(IdInstance::next_id());
    let second_user = User::random(IdInstance::next_id());

    //打印下用户信息
    // println!("first_user: {:?}", first_user);
    //print maps
    // world.print_map();

    //把用户加入到世界中
    world.add_user(first_user);
    world.add_user(second_user);
    let tick_time = world.tick_time;
    //-------------------------
    debug!("world init success");

    let world_counter = Arc::new(Mutex::new(world));
    debug!("world_counter create success");

    // need to assign on cluster mode
    let mut server_context = ServerContext::new_by_config(config.clone());

    let (to_client_sender, mut to_client_receiver): (Sender<ZmqMessage>, Receiver<ZmqMessage>) =
        tokio::sync::mpsc::channel(100);
    server_context.to_client_sender = to_client_sender;

    // create mutex for server_context
    let server_ctx = Arc::new(Mutex::new(server_context));

    let mut client_id_opt = None;

    if config.app.mode == Mode::Cluster {
        debug!("run in cluster mode");
        let client_id = do_in_cluster_mode(&server_ctx, config.clone()).await;

        // report_socket_opt = Some(to_registry_center_socket);
        client_id_opt = Some(client_id);

        // if slave node,do in slave
        if server_ctx.lock().await.node_type == NodeType::Slave {
            do_slave(&server_ctx, &world_counter).await;
        }
    }
    //----------------------初始化后,ServerContext已经是初始化后的可用状态-------------------

    // 这里需要多线程 世界刷新与其他操作不相关(查询,登录,更新用户的速度,位置等)
    let world_update_handle = tokio::spawn(world_update_thread(
        Arc::clone(&world_counter),
        tick_time,
        server_ctx.clone(),
        client_id_opt,
    ));

    // hashmap; key: client_id, value: last_heartbeat_time(milliseconds)
    let heartbeat_last_time: HashMap<String, i64> = HashMap::new();
    let heartbeat_last_time = Arc::new(tokio::sync::Mutex::new(heartbeat_last_time));
    let monitor_heartbeat_last_time = Arc::clone(&heartbeat_last_time);
    tokio::spawn(heartbeat_monitor(
        monitor_heartbeat_last_time,
        config.basic.heartbeat,
        Arc::clone(&world_counter),
        server_ctx.clone(),
    ));

    tokio::spawn(heartbeat_update(
        heartbeat_last_time,
        config.node.heartbeat_port,
    ));

    if config.test.overload_test.enable {
        // 节点负载测试
        tokio::spawn(load_test(
            Arc::clone(&world_counter),
            config.test.overload_test.max_user_count,
        ));
    }

    let test_world_counter = Arc::clone(&world_counter);
    tokio::spawn(main_channel_thread(
        server_ctx,
        world_counter,
        to_client_receiver,
        config.node.main_channel_port,
    ));

    (test_world_counter, world_update_handle)
}
//单节点模式和作为master节点时需要进行的操作
pub async fn world_update_only_master_or_single(
    world_update_couter: &Arc<Mutex<World>>,
) -> StateInfo {
    //todo:这里会死锁拿不到嘛?
    let mut world = world_update_couter.lock().await;

    // 定时更新世界
    world.update();
    world.state_info.clone()
}

//test_update_cost_time
pub async fn sleep_by_world_update_cost_time(tick_time: f32, start_time: time::Instant) -> f32 {
    //print cost time
    let cost_time = start_time.elapsed();

    //计算需要sleep的时间差
    let time_difference = tick_time - cost_time.as_secs_f32();
    // println!("time_difference : {:?}", time_difference);
    if time_difference > 0.0 {
        sleep(std::time::Duration::from_secs_f32(time_difference)).await;
    } else {
        //单次达到负载上限,打印信息,不sleep
        debug!(
            "single world overload,time_difference : {:?}",
            time_difference
        );
    }
    //世界延迟cost_time转为毫秒返回
    cost_time.as_millis() as f32
}

/// 计算世界update耗时,取最近100次world_update的平均值
/// 这个线程也会一直存在,不会因为主从切换,更换master停止重启
async fn world_update_thread(
    world_update_couter: Arc<Mutex<World>>,
    timer_clock: f32,
    server_ctx_counter: Arc<Mutex<ServerContext>>,
    client_id: Option<String>,
) {
    debug!("world update threadstart");
    let is_cluster = server_ctx_counter.lock().await.is_cluster();
    let to_registry_center_sender = server_ctx_counter
        .lock()
        .await
        .to_registry_center_sender
        .clone();

    let client_id = client_id.unwrap_or("".to_string());
    // 创建一个存储可存储100次的世界更新时间的链表(方便头部删除,尾部插入)
    let mut world_update_cost_time: VecDeque<f32> = VecDeque::new();
    let tick_time_millis = timer_clock * 1000.0;

    loop {
        //caculate cost time for sleep
        let start_time = time::Instant::now();
        //计算平均world_update_cost_time(链表数据的和/链表长度)
        let mut average_world_update_cost_time = 0.0;
        if !world_update_cost_time.is_empty() {
            average_world_update_cost_time =
                world_update_cost_time.iter().sum::<f32>() / world_update_cost_time.len() as f32;
            //判断平均延迟是否超过阈值
            if average_world_update_cost_time > tick_time_millis {
                error!(
                    "world overload,average_world_update_cost_time : {:?}",
                    average_world_update_cost_time
                );
            }
        }
        if is_cluster {
            let server_ctx = server_ctx_counter.lock().await;
            let sync_sender = server_ctx.world_sync_sender.clone();
            let latest_state_info = match server_ctx.node_type {
                // master节点,需要更新世界
                NodeType::Master => {
                    let mut state_info =
                        world_update_only_master_or_single(&world_update_couter).await;
                    //caculate sync cost time
                    // let sync_cost_time = time::Instant::now();
                    let maps = world_update_couter.lock().await.maps.clone();
                    // todo:集群的master从pub_socket同步世界数据给其他节点
                    // todo:死锁了,需要优化: 直接丢一个sender引用进去,搞一个pub线程等待recv到data then publish
                    tokio::spawn(sync_world_users_task(maps, sync_sender));
                    // let sync_cost_time = sync_cost_time.elapsed();
                    // debug!("sync_cost_time: {:?}", sync_cost_time);
                    state_info.avg_update_delay = average_world_update_cost_time;
                    state_info.clone()
                }
                NodeType::Slave => {
                    // slave节点,不需要更新世界,但是需要返回世界的状态信息(不用update,同步master世界的时候会自动更新一次)
                    // world_update_couter.lock().await.update_state_info();
                    // debug!("slave world update stateinfo");
                    // world.update_state_info();
                    // let sync_cost_time = time::Instant::now();

                    let mut state_info = world_update_couter.lock().await.state_info.clone();
                    state_info.avg_update_delay = average_world_update_cost_time;
                    // let sync_cost_time = sync_cost_time.elapsed();

                    // debug!("slave get state_info sync_cost_time: {:?}", sync_cost_time);
                    state_info
                }
            };

            //集群模式下需要report_load_factor
            tokio::spawn(try_report_load_factor_task(
                to_registry_center_sender.clone(),
                client_id.clone(),
                latest_state_info.clone(),
            ));
            // debug!("state_info: {:?}", latest_state_info.clone());
        } else {
            // 单机模式,直接更新世界与stateinfo,但是不需要report
            let mut latest_state_info =
                world_update_only_master_or_single(&world_update_couter).await;
            latest_state_info.avg_update_delay = average_world_update_cost_time;
            // debug!("state_info: {:?}", latest_state_info.clone());
        };

        let cost_time = sleep_by_world_update_cost_time(timer_clock, start_time).await;
        //加入计算链表(链表如果满了,移除最早的一个)
        world_update_cost_time.push_back(cost_time);
        if world_update_cost_time.len() > 100 {
            world_update_cost_time.pop_front();
        }
        // drop(world_update_couter)
    }
}

/// 节点负载测试 添加指定数量用户(负载测试) todo:后续单独做一个测试的方便操作的程序
async fn load_test(world_counter: Arc<Mutex<World>>, max_user_count: u32) {
    for _ in 0..max_user_count {
        // sleep(std::time::Duration::from_secs_f32(1.0));
        // 每过一秒往世界中添加一个用户
        let mut world = world_counter.lock().await;
        add_user2_world(&mut world, UserType::Global);
    }
}
#[cfg(False)]
mod tests {
    use std::time::Duration;

    use common::basic::{RectScope, Vector2};
    use tokio::time::sleep;

    use crate::server_run;

    #[tokio::test]
    async fn test_full_world_query() {
        // let mut world = super::World::init(1.0);
        let (world, _) = server_run::run().await;

        //全图
        let scope = RectScope {
            left_up: Vector2 {
                x: -1000000.0,
                y: 1000000.0,
            },
            right_down: Vector2 {
                x: 1000000.0,
                y: -1000000.0,
            },
        };
        sleep(Duration::from_secs(1)).await;
        let start = std::time::Instant::now();
        let user_count = world.lock().await.query(scope).len();
        println!("user_count :{:?}", user_count);
        println!("query time:{:?}", start.elapsed());
        //6000个用户全图单次查询5ms
        //20000 11ms
    }
    #[tokio::test]
    async fn test_scope_query() {
        // let mut world = super::World::init(1.0);
        let (world, _) = server_run::run().await;

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
        sleep(Duration::from_secs(1)).await;
        let start = std::time::Instant::now();
        let user_count = world.lock().await.query(scope).len();
        println!("user_count :{:?}", user_count);
        println!("query time:{:?}", start.elapsed());
    }

    #[tokio::test]
    async fn test_world_aoe() {
        // let mut world = super::World::init(1.0);
        let (world, _) = server_run::run().await;

        //等用户加进来
        sleep(Duration::from_secs(5)).await;
        let start = std::time::Instant::now();
        // 用户1001的aoe 周围40米
        world.lock().await.aoe(1001, 50.0, 100000);
        println!("aoe time:{:?}", start.elapsed());
    }
}
