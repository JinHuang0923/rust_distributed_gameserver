#!/bin/bash
ps -ef | grep target/debug/client | awk '{print $2}' | xargs kill -9