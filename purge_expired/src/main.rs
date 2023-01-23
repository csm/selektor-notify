mod lib;

use aws_config::meta::region::RegionProviderChain;
use aws_lambda_events::event::cloudwatch_events::CloudWatchEvent;
use aws_sdk_dynamodb as ddb;
use aws_sdk_dynamodb::model::AttributeValue;
use lambda_runtime::{run, service_fn, Error, LambdaEvent};
use std::collections::HashMap;
use std::env;
use std::time::SystemTime;
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        // disable printing the name of the module in every log line.
        .with_target(false)
        // disabling time is handy because CloudWatch will add the ingestion time.
        .without_time()
        .init();

    run(service_fn(lib::function_handler)).await
}
