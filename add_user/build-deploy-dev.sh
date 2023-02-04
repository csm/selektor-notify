#!/usr/bin/env bash

set -e
cargo build --release --target aarch64-unknown-linux-musl
cp ../target/aarch64-unknown-linux-musl/release/add_user bootstrap
zip add_user.zip bootstrap
aws lambda --region us-west-2 update-function-code --function-name add_user_dev --zip-file fileb://add_user.zip