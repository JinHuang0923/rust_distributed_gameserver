#!/bin/bash
ps -ef | grep 'cargo test' | awk '{print $2}' | xargs kill -9