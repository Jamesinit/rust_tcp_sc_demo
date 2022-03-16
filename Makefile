all: server client

server:
	cd tcp_server && cargo build --release

client:
	cd tcp_client && cargo build --release

test: server client
	cp tcp_server/target/release/tcp_server aitrans-server/bin/server
	cp tcp_client/target/release/tcp_client aitrans-server/client
	cd aitrans-server && ./run_server.sh
	sleep 0.1
	cd aitrans-server && ./run_client.sh

image_build:
	sudo docker build . -t simonkorl0228/qoe_test_image:9.0.1

library: tcp_server/demo/solution.cxx tcp_server/demo/solution.hxx
	cd tcp_server/demo && g++ -shared -fPIC solution.cxx -I. -o libsolution.so
kill:
	./aitrans-server/kill_server.sh
