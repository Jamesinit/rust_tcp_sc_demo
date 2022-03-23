# Rust 编写 TCP 测试程序

读取 DTP 的数据块格式文件，使用 TCP 进行数据发送测试。

`make test`即可进行本地测试

dtp_utils 中包含一些可以处理 dtp_config 相关的操作函数。

使用`make image_test_build`可以将在 ubuntu 20.04 编译的可执行程序复制到镜像里进行测试，可以节约大量编译时间。

## 简单原理说明

### 发送端 tcp_server

使用 dtp_utils 库中提供的 API 读取 DTP config 格式的文件，并且保存在结构体中。

使用一个 mio poll 来发送数据。大致的原理是：

1. 如果没有建立 TCP 连接则建立 TCP 连接
2. 如果建立了 TCP 连接则记录一个开始时间。如果一个块根据计算`send_time_gap`需要被发送，但是 socket 无法进行写，则视为将其放入一个队列中（实际上没有数据结构维护，只需要通过一开始的数组进行维护即可）。
3. 如果可以 socket 可以进行写操作，则从队列头开始发送数据块。每个数据块的前 40B 用来发送与块有关的一些信息，其会通过客户端的`StreamParser`进行解析。剩下的部分由全零的数据填充而成。
4. 如果 socket 的写操作完成后依然可以继续发送，则尝试继续发送。如果队列已经空了则空转等待。如果`write`函数报错`WouldBlock`，说明数据已经无法进行发送，此时会保存当前发送的块的信息并且推出发送循环，等待下一个`writable`事件发生。

### 接收端 tcp_client

接收端使用一个循环数组缓存接收到的数据，并且在每次接收到数据流后尝试从中解析出最多的数据块。循环数组的实现在`loopbytes.rs`中，解析器的实现在`streamparser.rs`中。所有被解析出的块会被打印出来。

## 使用方法样例

- server: `LD_LIBRARY_PATH=./lib ./bin/server 127.0.0.1 5555 'trace/block_trace/aitrans_block.txt' &> ./log/server_err.log &` 实际上并不需要 LD_LIBRARY_PATH 参数
- client: `LD_LIBRARY_PATH=./lib RUST_LOG=trace ./client 127.0.0.1 5555 --no-verify &> client_err.log &` 实际上不需要 LD_LIBRARY_PATH 参数

## 镜像文件说明

- dockerfile: 可以在容器中进行编译并且复制可执行文件。一般生成的都是 debian:buster 的镜像。可以使用`make image_build`来编译这种镜像。
- dockerfile_base: 如果想要在本地编译并生成镜像进行测试，那么首先使用一个相同操作系统并且安装了必要软件的镜像。可以参考这个dockerfile来写。这个文件生成的镜像会被下一个使用。
- dockerfile_test: 如果想在本地编译并生成镜像进行测试，那么首先使用`dockerfile_base`生成一个叫做`test_base`的镜像，然后在`dockerfile_test`中生成镜像，即可把aitrans-server目录下的东西直接粘贴过来，节约不少的编译时间。