use aws_config::meta::region::RegionProviderChain;
use aws_lambda_events::event::cloudwatch_events::CloudWatchEvent;
use aws_sdk_dynamodb as ddb;
use aws_sdk_dynamodb::model::AttributeValue;
use lambda_runtime::{run, service_fn, Error, LambdaEvent};
use std::collections::HashMap;
use std::env;
use std::time::SystemTime;
use tokio_stream::StreamExt;

const ENTITLEMENTS_TABLE_NAME: &str = "ENTITLEMENTS_TABLE_NAME";
const SCHEDULE_TABLE_NAME: &str = "SCHEDULE_TABLE_NAME";
const PARTITION_ID: &str = "PARTITION_ID";
const DYNAMODB_ENDPOINT: &str = "DYNAMODB_ENDPOINT";

async fn delete_schedules(ddb_client: &ddb::Client, item: &HashMap<String, AttributeValue>) -> Result<(), Error> {
    let schedule_table_name = env::var(SCHEDULE_TABLE_NAME)?;
    let partition_id = env::var(PARTITION_ID)?;
    if let Some(id) = item.get("id") {
        match id {
            AttributeValue::S(idval) => {
                let mut schedules = ddb_client.query()
                    .set_table_name(Some(schedule_table_name.clone()))
                    .set_index_name(Some(String::from("entitlement-index")))
                    .set_key_condition_expression(Some(String::from("#part = :part AND #ent = :ent")))
                    .set_expression_attribute_names(
                        Some(
                            HashMap::from([
                                (String::from("#part"), String::from("part")),
                                (String::from("#ent"), String::from("entitlement"))
                            ])
                        )
                    )
                    .set_expression_attribute_values(
                        Some(
                            HashMap::from([
                                (String::from(":part"), AttributeValue::S(partition_id)),
                                (String::from(":ent"), AttributeValue::S(idval.clone()))
                            ])
                        )
                    )
                    .into_paginator()
                    .send();
                while let Some(res) = schedules.next().await {
                    match res?.items() {
                        Some(i) => for item in i {
                            if let Some(part_val) = item.get("part") {
                                match &part_val {
                                    AttributeValue::S(part) => {
                                        if let Some(id_val) = item.get("id") {
                                            match &id_val {
                                                AttributeValue::S(id) => {
                                                    ddb_client.delete_item()
                                                        .set_table_name(Some(schedule_table_name.clone()))
                                                        .set_key(Some(
                                                            HashMap::from([
                                                                (String::from("part"), AttributeValue::S(part.to_string())),
                                                                (String::from("id"), AttributeValue::S(id.to_string()))
                                                            ])
                                                        ))
                                                        .send()
                                                        .await?;
                                                },
                                                _ => println!("unexpected value for id {:#?}", id_val)
                                            }
                                        }
                                    },
                                    _ => println!("unexpected value for part {:#?}", part_val)
                                }
                            }
                        },
                        None => break
                    }
                }
            },
            _ => println!("item id not a string {:#?}", item)
        }
    }
    Ok(())
}

pub async fn function_handler(_event: LambdaEvent<CloudWatchEvent>) -> Result<(), Error> {
    // Extract some useful information from the request
    let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
    let config = aws_config::from_env().region(region_provider).load().await;
    let ddb_config = match env::var(DYNAMODB_ENDPOINT) {
        Ok(endpoint) => ddb::config::Builder::from(&config).endpoint_url(endpoint).build(),
        _ => ddb::config::Builder::from(&config).build()
    };
    let ddb_client = ddb::Client::from_conf(ddb_config);
    let entitlements_table_name = env::var(ENTITLEMENTS_TABLE_NAME)?;
    let now = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_millis(),
        Err(_) => 0
    };
    let partition_id = env::var(PARTITION_ID)?;
    let mut expired = ddb_client.query()
        .set_table_name(Some(entitlements_table_name))
        .set_index_name(Some(String::from("ends-index")))
        .set_key_condition_expression(Some(String::from("#part = :part AND #ends < :now")))
        .set_expression_attribute_names(
            Some(
                HashMap::from([
                    (String::from("#part"), String::from("part")),
                    (String::from("#ends"), String::from("ends"))
                ])
            )
        )
        .set_expression_attribute_values(
            Some(
                HashMap::from([
                    (String::from(":part"), AttributeValue::S(partition_id.clone())),
                    (String::from(":now"), AttributeValue::N(now.to_string()))
                ])
            )
        )
        .into_paginator()
        .send();
    while let Some(res) = expired.next().await {
        match res?.items() {
            Some(i) => for item in i {
                delete_schedules(&ddb_client, item).await?;
            },
            None => break
        }
    }
    Ok(())
}