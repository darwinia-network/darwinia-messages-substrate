#!/bin/bash

DIR="$( cd "$( dirname "$0" )" && pwd )"
REPO_PATH="$( cd "$( dirname "$0" )" && cd ../ && pwd )"

EXECUTABLE=$REPO_PATH/target/release/substrate-relay

MILLAU_HOST=127.0.0.1
MILLAU_ALICE_PORT=10044

echo "Send Millau > Rialto Messages"
echo "Initialize Millau > Rialto bridge"
${EXECUTABLE} \
  send-message millau-to-rialto \
  --source-host=$MILLAU_HOST\
	--source-port=$MILLAU_ALICE_PORT\
	--source-signer=//Alice \
	--target-signer=//Bob \
  --lane 00000000 \
  remark
