# Introduction
1. 此项目为GameServer项目,同时接受客户端连接以及客户端请求
2. 分布式下:app_tag决定哪个是master,master处理所有的写请求,同时也处理客户端请求,但是与集群中的其他slave保持连接,每次写入新的数据到world时,将数据(world)同步广播到所有的slave.
3. master同注册中心也有连接,其余slave的连接信息都是从注册中心获取然后建立连接
4. master状态由注册中心维护,注册中心同时也是哨兵,如果master断开连接,则会从slave中选举出一个新的master(选举机制由节点负载量决定,可用节点中,负载量小的成为新的master)
5. 如果有节点挂了,客户端重连时,会从注册中心获取新的节点信息,然后重新连接(重连由客户端发起)
6. 注册中心监测到有非master节点挂了,则会将其从可用节点列表中删除,同时通知master,由master将其从slave列表中删除,同时记录日志
# 通信层
> 进程通信使用zmq
> 通信格式使用json 包含内容信息msg(String) 与client_id(u64),客户端与服务端的通信格式相同,客户端都会有一个唯一的client_id(uuid)
## 通信保障
1. 心跳,客户端与服务端都会定时发送心跳,如果服务端在一定时间内没有收到客户端的心跳,则会认为客户端已经断开连接,并将其从客户端列表中删除
2. 客户端没有收到服务端的心跳,则会认为服务端已经断开连接,并尝试重新连接
# TODO
- [ ] 命令行参数读取配置文件的位置(不写死了)
- [ ] world从互斥锁改为读写锁,可提升并发读的性能
- [x] login实现 
  - [x]login默认不带速度
  - [x] 实现set_velocity 

- [x] aoe单机实现
  - [x] aoe接上客户端操作
- [x] 负载量测试,世界延迟
  - [x] 单世界更新
  - [ ] 世界更新加模拟用户所有用户同时随机范围query,所有用户同时随机aoe(50m内)
  - [ ] 在上述压力测试下,找到tick_time=0.02s时,单节点可承载的最大用户数

- [x] 世界更新时,超出当前map的范围,需要重新处理加入到其他map
- [x] get_cover_map_by_scope 获取中间部分的map(为x,ylen加上符号可能更好解决)
- [x] 为方便快速从world获取到用户,world本身需要一个map,用于存储用户id与用户的映射关系,这个需要与地图中的保持同步 (remove_user bug修复)
- [x] 解决心跳连接客户端断开后,服务端心跳线程panic的问题
  - 通过降低ping的频率解决这个问题
- [x] 使用tracing替换println作为日志库
- [x] query单点实现
- [x] query客户端查询实现(handle_msg)
  - [ ] 返回的查询结果优化为pb
- [x] heartbeat拆分为crate后续给regitry_center也可以用上
  - [ ] after_offline优化为传入闭包
  - [ ] heartbeat使用的zmqcrate替换为修改过的
- [ ] 返回给客户端的消息都使用pb格式，定义返回消息的结构体
# 分布式
- [ ] 注册中心挂了之后程序本身不要挂掉,而是等待注册中心恢复,或是切换备用节点 
- [ ] pub/sub在世界更新时,进行world数据同步(map) 
- [x] main_handle处理时,除了login外,其他的可能需要转发给master的操作都淡掉client_id,使用user_id作为状态
- [ ] pb序列化maps测试
- [x] world_update 更新stateinfo, 发送到注册中心
- [ ] 根据主从mode的不同,world_update时 从server并不调用实际的world_update
- [ ] 根据主从不同,从server需要单独一个进程接收来自master的world_update消息,并更新自己的world
- [ ] 从server下处理部分客户端operation时,将操作转发给master,由master处理后再返回给客户端(因为master没有对应的client_id,所以转发前转发的必须是user_id)
  - [ ] user_id加为operation,写操作必须携带的参数(login时返回给客户端user_id)
- [ ] master的注册信息额外多带一个pub的端口,用于slave订阅update_msg
- [ ] slave与master的world同步,为了减少数据量,slave只需要在world_update时同步,其他改变用户的操作不需要单独同步(这样会造成slave大概tick+数据传输时间的延迟)
- [ ] 准备3份配置文件,写一个部署脚本,部署一个master,两个slave,测试分布式的高可用(断联,重新选主)


- [ ] 单独开一个线程处理注册中心的指令
  - [ ] 本机转换为master
    - [ ] 更新tag world_update时根据type不同做出不同处理
  - [ ] 更换master
    - [ ] 重新订阅新的master的同步端口
    - [ ] 重新连接新的master的main处理端口
- [x] slave转发消息到master
- [x] slave转发response到client
- [x] slave订阅master的world_update消息,更新世界信息
!!!
- [x] server_ctx的pub_socket换成pub_sender,单独开一个线程处理pub消息
- [ ] cluster server启动时确保注册成功,否则直接程序结束
## 优化
- [ ] 哪些地方还可以使用异步task提升性能
- [x] server的handle_msg改为异步task 同时处理多个客户端消息
- [ ] query请求改为不需要登录也可以直接处理请求,这样master节点即使挂了,query请求仍然不受影响
## bug
- [x] 登录了但是query的数量没有增加(add_user有问题,不然就是没来得及同步,query晚一点试试)
- [ ] connection的数量不对,连接上的客户端即使不登录(仅query)也应该算一个连接,client_map仅仅包含了登录的客户端
## 可用测试
- [x] Single模式下可用
- [x] Cluster模式,作为单Master节点可用
- [ ] Cluster模式,一主多从可用
  - [x] 不考虑主从数据同步,conver_to_master,change_master操作
  - [x] 兼容主从数据同步,不考虑conver_to_master,change_master操作
  - [ ] 兼容主从数据同步,考虑conver_to_master,change_master操作
# client
- [ ] console操作界面
- [ ] 读取命令行参数
- [ ] 测试脚本,实际多开客户端,轮流无间断进行各个操作,测试服务端负载表现
- [ ] 服务端断开连接后,客户端自动重连(打印日志)
# 杂记
1. master 与 slave之间不需要心跳保持,master的状态由注册中心维护
