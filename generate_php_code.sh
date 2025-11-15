#!/usr/bin/env bash

# protoc-gen-grpc must be installed (see: https://grpc.io/docs/languages/php/basics)
mkdir -p target/proto/php
protoc --proto_path=proto \
  --php_out=target/proto/php \
  --grpc_out=target/proto/php \
  --plugin=protoc-gen-grpc=/usr/local/bin/grpc_php_plugin \
  ./proto/vecembed.proto