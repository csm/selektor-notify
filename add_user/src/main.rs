mod lib;

use std::env;
use std::fmt::{Display, Formatter};
use std::ops::Add;
use lambda_http::{run, service_fn, Body, Error, Request, RequestExt, Response};
use serde::{Deserialize, Serialize};
use serde_json;
use add_user::{AddUserRequest, add_user};

async fn function_handler(event: Request) -> Result<Response<Body>, Error> {
    let request: serde_json::Result<AddUserRequest> = match event.body() {
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

    match request {
        Ok(request) => {
            let response = add_user(request).await;
            match response {
                Ok(r) => Ok(
                    Response::builder()
                        .status(200)
                        .header("content-type", "application/json")
                        .body(serde_json::to_string(&r)?.into())
                        .map_err(Box::new)?
                ),
                Err(e) => match e {
                    _ => {
                        println!("error adding user: {}", e);
                        Ok(Response::builder()
                            .status(500)
                            .header("content-type", "text/plain")
                            .body(format!("{}", e).into())
                            .map_err(Box::new)?
                        )
                    }
                }
            }
        },
        Err(e) => {
            println!("error parsing body: {}", e);
            Ok(
                Response::builder()
                .status(400)
                .header("content-type", "application/json")
                .body(format!("{{\"error\":\"{}\"}}", e).into())
                .map_err(Box::new)?
            )
        }
    }
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
