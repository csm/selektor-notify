mod lib;

use lambda_http::{run, service_fn, Body, Error, Request, RequestExt, Response};
use lambda_http::request::RequestContext;
use serde_json::Value;
use tracing::info;
use register_push::{RegisterPushRequest, register_push};

/// This is the main body for the function.
/// Write your code inside it.
/// There are some code example in the following URLs:
/// - https://github.com/awslabs/aws-lambda-rust-runtime/tree/main/examples
async fn function_handler(event: Request) -> Result<Response<Body>, Error> {
    info!("register_push event: {:?}, context: {:?}", event, event.request_context());
    let resp = match event.request_context() {
        RequestContext::ApiGatewayV1(ctx) => {
            match ctx.authorizer.get("principalId") {
                Some(Value::String(principal)) => {
                    let request: serde_json::Result<RegisterPushRequest> = match event.body() {
                        Body::Text(s) => serde_json::from_str(s),
                        Body::Binary(b) => serde_json::from_slice(b),
                        Body::Empty => return Ok(
                            Response::builder()
                                .status(400)
                                .header("content-type", "text/plain")
                                .body("Expected a request body.".into())
                                .map_err(Box::new)?
                        )
                    };

                    info!("register_push {:?}", request);
                    register_push(principal, request?).await?;
                    Response::builder()
                        .status(204)
                        .body(Body::Empty)
                        .map_err(Box::new)?
                },
                _ => Response::builder()
                    .status(401)
                    .header("content-type", "text/plain")
                    .body("please authenticate".into())
                    .map_err(Box::new)?
            }

        }
        _ => Response::builder()
            .status(500)
            .header("content-type", "text/plain")
            .body("Internal Server Error".into())
            .map_err(Box::new)?
    };
    Ok(resp)
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        // disable printing the name of the module in every log line.
        .with_target(false)
        // disabling time is handy because CloudWatch will add the ingestion time.
        .without_time()
        .init();

    run(service_fn(function_handler)).await
}
