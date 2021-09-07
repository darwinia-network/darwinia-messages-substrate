#!/bin/bash

DIR="$( cd "$( dirname "$0" )" && pwd )"
REPO_PATH="$( cd "$( dirname "$0" )" && cd ../ && pwd )"

MILLAU_HOST=127.0.0.1
MILLAU_ALICE_PORT=10044
RIALTO_HOST=127.0.0.1
RIALTO_ALICE_PORT=10144

echo "Build substrate relayer"
cargo build -p substrate-relay --release

echo ""
EXECUTABLE=$REPO_PATH/target/release/substrate-relay

echo "Run header-message relay"
${EXECUTABLE} \
  

