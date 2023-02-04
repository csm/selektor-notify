use std::collections::HashMap;
use aws_sdk_dynamodb as ddb;
use aws_sdk_sns as sns;
use lambda_http::Error;
use serde::{Deserialize, Serialize};
use std::env;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_dynamodb::model::AttributeValue;
use lambda_http::aws_lambda_events::http_body::Body;

const PUSH_TABLE_NAME: &str = "PUSH_TABLE_NAME";
const DYNAMODB_ENDPOINT: &str = "DYNAMODB_ENDPOINT";
const SNS_APP_ARN: &str = "SNS_APP_ARN";

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterPushRequest {
    push_token: String
}

pub async fn register_push(principal: &String, request: RegisterPushRequest) -> Result<(), Error> {
    let table_name = env::var(PUSH_TABLE_NAME)?;
    let sns_app_arn = env::var(SNS_APP_ARN)?;

    let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
    let config = aws_config::from_env().region(region_provider).load().await;
    let ddb_config = match env::var(DYNAMODB_ENDPOINT) {
        Ok(endpoint) => ddb::config::Builder::from(&config).endpoint_url(endpoint).build(),
        _ => ddb::config::Builder::from(&config).build()
    };
    let ddb_client = ddb::Client::from_conf(ddb_config);
    let sns_client = sns::Client::new(&config);

    let existing_data = ddb_client.get_item()
        .set_table_name(Some(table_name.to_owned()))
        .set_key(Some(HashMap::from([("id".to_string(), AttributeValue::S(principal.to_string()))])))
        .send()
        .await?;

    match existing_data.item() {
        Some(item) => {
            match item.get("endpoint_arn") {
                Some(AttributeValue::S(arn)) => {
                    let endpoint = sns_client.get_endpoint_attributes()
                        .set_endpoint_arn(Some(arn.to_owned()))
                        .send()
                        .await?;
                    let delete = match endpoint.attributes.and_then(|m| { m.get("Token").cloned() }) {
                        Some(token) => if token.eq(&(request.push_token)) {
                            // Token already exists, skip anything else.
                            return Ok(())
                        } else {
                            true
                        },
                        None => false
                    };
                    if delete {
                        sns_client.delete_endpoint()
                            .set_endpoint_arn(Some(arn.to_owned()))
                            .send()
                            .await?;
                    }
                }
                _ => {}
            }
        }
        None => {}
    }

    let endpoint_result = sns_client.create_platform_endpoint()
        .set_platform_application_arn(Some(sns_app_arn))
        .set_token(Some(request.push_token))
        .send()
        .await?;

    match endpoint_result.endpoint_arn() {
        Some(arn) => {
            ddb_client.put_item()
                .set_table_name(Some(table_name.to_owned()))
                .set_item(Some(HashMap::from([
                    ("id".to_string(), AttributeValue::S(principal.to_owned())),
                    ("endpoint_arn".to_string(), AttributeValue::S(arn.to_string()))
                ])))
                .send()
                .await?;
        }
        None => {}
    }

    Ok(())
}