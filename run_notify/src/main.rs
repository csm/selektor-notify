mod lib;

use lambda_runtime::{Error, run, service_fn};
use std::env;

const TRACING_DEBUG: &str = "TRACING_DEBUG";

#[tokio::main]
async fn main() -> Result<(), Error> {
    let tracing_result = env::var(TRACING_DEBUG);
    tracing_subscriber::fmt()
        .with_max_level(if let Ok(_) = tracing_result {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        })
        // disable printing the name of the module in every log line.
        .with_target(false)
        // disabling time is handy because CloudWatch will add the ingestion time.
        .without_time()
        .init();

    run(service_fn(lib::function_handler)).await
}
