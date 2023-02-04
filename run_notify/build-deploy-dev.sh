#!/usr/bin/env bash

set -e
cargo build --release --target aarch64-unknown-linux-musl
cp ../target/aarch64-unknown-linux-musl/release/run_notify bootstrap
zip run_notify.zip bootstrap
aws lambda --region us-west-2 update-function-code --function-name run_notify_dev --zip-file fileb://run_notify.zip