#!/bin/bash
LD_LIBRARY_PATH=./lib RUST_LOG=trace ./client 127.0.0.1 5555 --no-verify &> client_err.log &
