FROM simonkorl0228/aitrans_build:buster as build
WORKDIR /build
COPY ./tcp_client ./tcp_client
COPY ./tcp_server ./tcp_server
COPY ./dtp_utils ./dtp_utils
COPY ./Makefile ./Makefile
RUN echo "[source.crates-io]\n\
    replace-with = 'tuna'\n\n\
    [source.tuna]\n\
    registry = \"https://mirrors.tuna.tsinghua.edu.cn/git/crates.io-index.git\"" > $CARGO_HOME/config && \
    make

FROM simonkorl0228/aitrans_image_base:buster
COPY --from=build \
    /build/tcp_server/target/release/tcp_server /home/aitrans-server/bin/server
COPY --from=build \
    /build/tcp_client/target/release/tcp_client /home/aitrans-server/client
WORKDIR /home/aitrans-server