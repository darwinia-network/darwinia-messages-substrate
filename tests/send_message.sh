#!/bin/bash

# This script helps send Millau > Rialto message
# NOTE: Make sure the Millau/Rialto network and the Substrate-relayer running before sending messages.
#
# Usage: ./tests/send_message.sh 10 1

DIR="$( cd "$( dirname "$0" )" && pwd )"
REPO_PATH="$( cd "$( dirname "$0" )" && cd ../ && pwd )"

n=$1
s=$2

EXECUTABLE=$REPO_PATH/target/release/substrate-relay

MILLAU_HOST=127.0.0.1
MILLAU_ALICE_PORT=10044

index=0
while [ $index -lt $n ]
do
  echo "### Send Millau > Rialto Messages"
  ${EXECUTABLE} \
    send-message millau-to-rialto \
    --source-host=$MILLAU_HOST\
	  --source-port=$MILLAU_ALICE_PORT\
	  --source-signer=//Alice \
	  --target-signer=//Bob \
    --lane 00000000 \
    remark

  sleep $s
  ((index++))
done
