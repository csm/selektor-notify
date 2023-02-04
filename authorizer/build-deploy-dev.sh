#!/usr/bin/env bash

set -e
cargo build --release --target aarch64-unknown-linux-musl
cp ../target/aarch64-unknown-linux-musl/release/authorizer bootstrap
zip authorizer.zip bootstrap
aws lambda --region us-west-2 update-function-code --function-name authorizer --zip-file fileb://authorizer.zip