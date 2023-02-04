#!/usr/bin/env bash

set -e
cargo build --release --target aarch64-unknown-linux-musl
cp ../target/aarch64-unknown-linux-musl/release/register_push bootstrap
zip register_push.zip bootstrap
aws lambda --region us-west-2 update-function-code --function-name register_push_dev --zip-file fileb://register_push.zip