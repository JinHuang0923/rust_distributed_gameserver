# Introduction
`For gameserver,a demo that high-performance distributed system.implent with Rust.`
# Project Layout
## Client(only for test)  
1. 用于测试的客户端,可从注册中心自动获取合适的server链接信息,然后连接server进行各种指令操作.  
2. 单机server模式下,需要手动指定server的链接信息  
3. 与server保持着心跳检测,client关闭时,server会自动清除login的用户信息,server关闭时,client的心跳检测可以检测到,可以重新从注册中心获取最新的server链接信息进行重连(心跳线程也需要重新启动)    
## Server(GameServer)
1. 核心的gameserver,负责处理客户端的各种指令,并响应  
2. server通过配置文件,可选择性的以集群模式/单机模式启动  
3. 以集群模式启动时,server会自动注册到注册中心,并且会定时向注册中心发送心跳,以及负载状态(客户端连接数,负载率,世界用户,当前世界更新的延迟)  
4. 以master启动时,可以是单master模式,也可以启动更多的slave server node,master_node 会处理所有的写请求(login,logout,aoe,set_velocity),但是不会与client直接连接,处理的响应将会发送给slave,slave_node会将响应发送给指定的cient.每一次world_update时,master node都会将当前的世界状态发布更新出去.订阅的slave node自动更新自己的世界状态,与master node保持同步,slave的数据延迟在30ms内  
5. 以slave启动时,先注册节点到注册中心,然后拿到master的发布连接信息,订阅master的world_update,收到update时并且自动更新自己的世界状态,与master保持同步.slave会直接连接client,处理client的读请求(query)并直接响应,无需与master交互,其余的写请求,将会转发给master处理,并将响应发送给client     
## Registry Center
1. 注册中心,用于Server发现与注册,以及负载均衡,并可实时观测GameServer各节点的运行状态,负载率  
2. server可将连接信息在这里注册,并建立心跳连接,实时监控server的运行状态,以及负载.新扩容,加入集群的slave node 节点可在这里获取最新master的发布连接信息,并订阅master的world_update  
3. client链接server前,通过http请求获取合适的server链接信息,并建立连接.返回的node信息,是经过负载均衡的策略计算出的(默认策略是 与client connection负载率最低的节点)  
4. 保证高可用,当集群中的master节点意外宕机时,注册中心可以第一时间发现,并且进行选主,根据策略,选出最合适的可用节点转换为master,向对应节点发送convert_to_master指令,向其他slave节点发送change_master指令,通知其切换master节点,新生的master数据只会比原master节点的数据延迟30ms内(同步速度决定)  

# 其他说明
1. 通信实现
> 主要通信使用的是zmq,client2server 的 心跳检测,发起操作的长连接,server2注册中心的心跳检测,汇报,指令响应,以及master node与slave node的数据同步,请求转发.    
> 其次在无需高性能的地方,使用了少部分的http通信,获取负载均衡的连接信息,获取所有节点的信息,负载,健康状态,获取最新master节点信息等.    
2. 数据同步的格式以及通信的数据格式
> client2server,交互的信息是json格式,更加可读  
> master node 与 slave node 之间的数据同步,使用的数据格式为二进制,因为同步的数据量较大,并且序列化反序列耗时较长,使用二进制将数据同步的时间控制在30ms内  
# 需求分析与实现考量
1. 分布式系统的ACP限制,所以只能在根据具体需求,做不同侧重点的实现,包括一致性,可用性,延迟,负载量等,可更细分的是,强一致,还是可承受的最终一致,可用性是保证一直可用,还是需要stop world,保证所有结果完全一致,放弃高可用.延迟是写请求的延迟要求需要更低,还是读请求?负载量到多少才算合适  
2. 考虑本次实现的分布式系统是游戏服务端,所以低延迟,高可用是必须的,为此需要放弃一部分的一致性,slave节点的数据延迟30ms内,在fps 50的客户端的感受中,只会比直连master少1帧的延迟,属于可接受范围内.  
3. 低延迟的实现,读延迟跟写延迟可以对应做不同的分布式实现,本次的实现,是以读延迟更低为目标的,client的读请求(例query)无论从哪个节点都可以直接最快速的响应,并且为了query大面积的world也能快速响应,把整个世界分为了多个地图(100),可根据query范围,选择性的查询,减少查询的数据量,提高查询的速度.  
4. 关于写延迟,现在的实现由于写请求都要到master处理,进行一次转发,所以其他节点的客户端进行写请求,会比单机延迟要高.为了降低部分的写延迟,进行的分地图处理.这会牺牲一定的负载量.但是整个world都是一个整体,跨区域的操作也是会更快的,比如横跨两个地图/场景的aoe等  
# 性能测试结果(debug编译下的数据)
1. 这个方案实现时,优先级选择了读请求延迟最低,高可用性,负载量适中,一致性最终一致,不是负载量最高的实现方法,实现了需求上的query最快的需求(读延迟最低),其实可以做一个权衡,我在最后给出了另一种的实现方案,可以算是另一种,也可以是现有方案的优化版本  
2. 测试方式
   > 测试方式为,实开客户端连接进行每秒两次的请求,持续30s,读写都有.在连接都存在的情况下,单开客户端进行请求,并计算平均响应时间  
   > client的目录下有个multi_test.sh脚本,接受一个命令行参数,参数*5就是模拟客户端连接的数量,如模拟2000个连接,那么就是./multi_test.sh 400
3. 根据硬件性能不同(主要是cpu主频,内存频率),测试结果肯定有所差异,已下为开发机大概的测试结果
   1. 单机模式
      1. 305 query全图平均 45ms aoe/set_velocity 72ms
      2. 400 query全图平均 50ms aoe/set_velocity 165ms
   2. 集群多节点(客户端得真正到单节点压力水平才能看出来)(一主两从)
      1. 305个连接 query avg 2ms aoe/set_velocity 4ms
      2. 500个连接 query avg 2ms aoe/set_velocity 64ms
      3. 1000 query avg 2ms aoe/set_velocity 248ms
   3. 一主四从
      1. 1750 query avg 2ms aoe/set_velocity 80ms
   4. ....这个系统的性能极限是master能够在tick_time内处理请求的极限,0.02s能处理多少个多个slave多个用户发来的写请求,那么整个集群slave扩展到那么大就行,单机100倍(3000个客户端)应该是可以达到的,
      1. 五从已经可以在正常延迟内响应1750个客户端的请求了

# 基于高负载与写请求延迟降低需求的另一种实现
1. 一组server(一主多从)单个world,一个world只包含地图的一部分(比如上面分的100个的其中一个),server 注册时把连接信息与地图标识映射起来,存储在注册中心上
2. 多组server的运行模式与上方的基本一致,master写,slave转发写,处理读.只是分的地图范围变小了,所以负载量可以增大
3. query请求一定会比上方的模式慢,如果是涉及到多个地图的大范围,或者全图query,那么接收到请求的server需要向所有涉及到这个范围的map对应的server发起一次查询.(map标识与对应server的连接信息从注册中心取),范围越大需要的rpc请求越多,查询时间,最终响应时间越长
4. 如果忽略aoe的范围可能会同时影响到多个地图的user,那么aoe比上述方案要快一些(具体是少了转发rpc请求的时间,大概是10ms左右),如果不忽略的话,aoe的延迟最差情况(全分矩形最多影响到4个地图(包括自己))会比上述方案还要慢一些(异步发起3个rpc,但是需要等待响应,做最终的客户端响应)
5. set_velocity 比上述方案少一次转发rpc请求时间
6. !!!负载量可大大增加,理论上来说,地图能分多少个节点,那么负载量就能提升多少倍,但是(aoe,query)的响应时间会因为在线人数的多少变得越来越慢