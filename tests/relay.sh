#!/bin/bash

DIR="$( cd "$( dirname "$0" )" && pwd )"
REPO_PATH="$( cd "$( dirname "$0" )" && cd ../ && pwd )"
LOG_DIR=$DIR/logs/

MILLAU_HOST=127.0.0.1
MILLAU_ALICE_PORT=10044
RIALTO_HOST=127.0.0.1
RIALTO_ALICE_PORT=10144


echo "Build substrate relayer"
cargo build -p substrate-relay --release

EXECUTABLE=$REPO_PATH/target/release/substrate-relay

RUST_LOG=bridge=trace
export RUST_LOG

echo "Initialize Millau > Rialto bridge"
${EXECUTABLE} \
  init-bridge millau-to-rialto \
  --source-host=$MILLAU_HOST\
	--source-port=$MILLAU_ALICE_PORT\
	--target-host=$RIALTO_HOST\
	--target-port=$RIALTO_ALICE_PORT\
	--target-signer=//Alice &> $LOG_DIR/relay.log &


echo "Start header-message relay"
${EXECUTABLE} \
  relay-headers-and-messages millau-rialto \
  --millau-host=$MILLAU_HOST\
	--millau-port=$MILLAU_ALICE_PORT\
	--millau-signer=//Alice\
	--rialto-host=$RIALTO_HOST\
	--rialto-port=$RIALTO_ALICE_PORT\
	--rialto-signer=//Alice\
	--lane=00000000 &> $LOG_DIR/relay.log &
