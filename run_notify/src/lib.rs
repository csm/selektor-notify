use aws_config::meta::region::RegionProviderChain;
use aws_sdk_dynamodb as ddb;
use aws_sdk_dynamodb::model::AttributeValue;
use aws_sdk_sns as sns;
use aws_lambda_events::event::cloudwatch_events::CloudWatchEvent;
use lambda_runtime::LambdaEvent;
use lambda_runtime::Error;
use std::collections::HashMap;
use std::env;
use std::str::FromStr;
use std::time::{Duration, SystemTime};
use aws_sdk_sns::model::MessageAttributeValue;
use tokio_stream::StreamExt;
use tracing::{debug, error, info, warn};

const FIVE_MINUTES: Duration = Duration::from_secs(5 * 60);
const TABLE_NAME: &str = "TABLE_NAME";
const PARTITION_ID: &str = "PARTITION_ID";
const PUSH_TABLE_NAME: &str = "PUSH_TABLE_NAME";

pub async fn function_handler(event: LambdaEvent<CloudWatchEvent>) -> Result<(), Error> {
    let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
    let config = aws_config::from_env().region(region_provider).load().await;
    let ddb_client = ddb::Client::new(&config);
    let fire_time = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(n) => n.as_millis() / FIVE_MINUTES.as_millis(),
        Err(_) => 0
    };
    let next_fire_time = fire_time + 1;
    let sns_client = sns::Client::new(&config);
    let table_name = env::var(TABLE_NAME)?;
    let push_table_tame = env::var(PUSH_TABLE_NAME)?;
    let partition_id = env::var(PARTITION_ID)?;
    let mut results = ddb_client.query()
        .table_name(table_name.to_owned())
        .index_name("next_fire-index")
        .key_condition_expression("#part = :part_val AND #next_fire < :next_fire_val")
        .expression_attribute_names("#part", "part")
        .expression_attribute_names("#next_fire", "next_fire")
        .expression_attribute_values(":part_val", AttributeValue::S(partition_id.to_owned()))
        .expression_attribute_values(":next_fire_val", AttributeValue::N(next_fire_time.to_string()))
        .into_paginator()
        .send();
    while let Some(res) = results.next().await {
        match res?.items() {
            Some(i) => for item in i {
                if let Some(AttributeValue::S(id)) = item.get("id") {
                    debug!("looking at id={}", id);
                    let push = ddb_client.get_item()
                        .table_name(push_table_tame.to_string())
                        .key("id".to_string(), AttributeValue::S(id.to_owned()))
                        .send()
                        .await?;
                    if let Some(item) = push.item() {
                        if let Some(AttributeValue::S(arn)) = item.get("endpoint_arn") {
                            let publish_result = sns_client.publish()
                                .target_arn(arn)
                                .message("{\"APNS\":{\"aps\":{\"content-available\":1}}}")
                                .message_attributes(
                                    "AWS.SNS.MOBILE.APNS.PUSH_TYPE".to_string(),
                                    MessageAttributeValue::builder()
                                        .data_type("String")
                                        .string_value("background")
                                        .build()
                                )
                                .message_attributes(
                                    "AWS.SNS.MOBILE.APNS.PRIORITY".to_string(),
                                    MessageAttributeValue::builder()
                                        .data_type("String")
                                        .string_value("5")
                                        .build()
                                )
                                .send()
                                .await;
                            match publish_result {
                                Err(e) => error!("error publishing to {}: {}", arn, e),
                                Ok(_) => info!("send push for id: {}", id)
                            }
                        } else {
                            warn!("no endpoint_arn for push item: {:?}", item);
                        }
                    } else {
                        warn!("no push entry for id: {}", id);
                    }
                } else {
                    warn!("item with no id: {:?}", item);
                }
                if let Some(v) = item.get("fire_interval") {
                    match v {
                        AttributeValue::N(interval) => {
                            if let Some(id) = item.get("id") {
                                match id {
                                    AttributeValue::S(idval) => {
                                        match u128::from_str(interval.as_str()) {
                                            Ok(i) => {
                                                let next = fire_time + i;
                                                ddb_client.update_item()
                                                    .table_name(table_name.to_owned())
                                                    .key("part", AttributeValue::S(partition_id.to_owned()))
                                                    .key("id", AttributeValue::S(idval.to_string()))
                                                    .update_expression("SET #fire = :i")
                                                    .expression_attribute_names("#fire", "next_fire")
                                                    .expression_attribute_values(":i", AttributeValue::N(next.to_string()))
                                                    .send()
                                                    .await?;
                                            },
                                            Err(_) => error!("couldn't parse number: {:#?}", interval)
                                        }
                                    },
                                    _ => error!("invalid ID in item {:#?}", item)
                                }
                            } else {
                                error!("no id for item: {:#?}", item)
                            }
                        }
                        _ => error!("ignoring non-number fire_interval {:#?}", v)
                    }
                }
            },
            None => break
        }
    }
    Ok(())
}