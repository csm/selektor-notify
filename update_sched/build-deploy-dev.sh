#!/usr/bin/env bash

set -e
cargo build --release --target aarch64-unknown-linux-musl
cp ../target/aarch64-unknown-linux-musl/release/update_sched bootstrap
zip update_sched.zip bootstrap
aws lambda --region us-west-2 update-function-code --function-name update_sched_dev --zip-file fileb://update_sched.zip