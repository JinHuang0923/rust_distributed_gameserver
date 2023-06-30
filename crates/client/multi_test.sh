#!/bin/bash
#count从命令行获取
count=$1
# 循环100次执行test,每一个循环会同时存在5个connection到server(四个登录了)
for((i=1;i<=$count;i++));
do
    cargo test &
done
