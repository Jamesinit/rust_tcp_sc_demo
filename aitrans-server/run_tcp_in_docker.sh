#!/bin/bash

# 默认地址和端口
DEFAULT_ADDRESS="127.0.0.1"
DEFAULT_PORT=5555

# 判断用户是否提供了地址和端口
if [ "$1" == "-s" ]; then
    if [ -n "$2" ] && [ -n "$3" ]; then
        SERVER_ADDRESS="$2"
        SERVER_PORT="$3"
    else
        SERVER_ADDRESS="$DEFAULT_ADDRESS"
        SERVER_PORT="$DEFAULT_PORT"
    fi
    LD_LIBRARY_PATH=./lib RUST_BACKTRACE=1 RUST_LOG=debug ./bin/tcp_server "$SERVER_ADDRESS" "$SERVER_PORT" 'trace/block_trace/aitrans_block.txt' &> ./log/tcp_server_err.log &
    echo "Server 已启动"
elif [ "$1" == "-c" ]; then
    if [ -n "$2" ] && [ -n "$3" ]; then
        CLIENT_ADDRESS="$2"
        CLIENT_PORT="$3"
    else
        CLIENT_ADDRESS="$DEFAULT_ADDRESS"
        CLIENT_PORT="$DEFAULT_PORT"
    fi
    LD_LIBRARY_PATH=./lib RUST_LOG=trace ./tcp_client "$CLIENT_ADDRESS" "$CLIENT_PORT" --no-verify &> tcp_client_err.log 
    echo "运行结束，请查看 tcp_client.log"
elif [ "$1" == "test" ]; then
    if [ -n "$2" ] && [ -n "$3" ]; then
        SERVER_ADDRESS="$2"
        SERVER_PORT="$3"
    else
        SERVER_ADDRESS="$DEFAULT_ADDRESS"
        SERVER_PORT="$DEFAULT_PORT"
    fi
    LD_LIBRARY_PATH=./lib RUST_BACKTRACE=1 RUST_LOG=debug ./bin/tcp_server "$SERVER_ADDRESS" "$SERVER_PORT" 'trace/block_trace/aitrans_block.txt' &> ./log/tcp_server_err.log &
    sleep 0.1
    LD_LIBRARY_PATH=./lib RUST_LOG=trace ./tcp_client "$SERVER_ADDRESS" "$SERVER_PORT" --no-verify &> tcp_client_err.log 
    echo "运行结束，请查看 tcp_client.log"
else
    echo "用法：$0 [-s|-c|test] [地址] [端口]"
    echo "  -s  只启动 server"
    echo "  -c  只启动 client"
    echo "  test  启动 server 和 client，测试用"
fi
