
# First build the app with all build dependencies
FROM alpine:edge AS build-image
RUN apk add rust cargo openssl-dev
COPY . /opt/exocore/
RUN cd /opt/exocore/cli && \
    cargo install --path .

# Then copy app & required libs to a blank Alpine
FROM alpine
WORKDIR /app
COPY --from=build-image /usr/lib/libgcc* /usr/lib/
COPY --from=build-image /root/.cargo/bin/exocore-cli /app/
ENTRYPOINT [ "/app/exocore-cli" ]
