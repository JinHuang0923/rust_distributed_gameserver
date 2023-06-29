#!/bin/bash
count=$1
for ((i=0; i<$count; i++)) do
cargo run &
# echo "Starting client $i"
done