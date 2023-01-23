use aws_config::meta::region::RegionProviderChain;
use aws_sdk_dynamodb as ddb;
use aws_sdk_dynamodb::model::AttributeValue;
use aws_sdk_sns as sns;
use aws_lambda_events::event::cloudwatch_events::CloudWatchEvent;
use lambda_runtime::LambdaEvent;
use aws_lambda_events::serde_json::de::Read;
use lambda_runtime::Error;
use std::collections::HashMap;
use std::env;
use std::str::FromStr;
use std::time::{Duration, SystemTime};
use tokio_stream::StreamExt;

const FIVE_MINUTES: Duration = Duration::from_secs(5 * 60);
const TABLE_NAME: &str = "TABLE_NAME";
const PARTITION_ID: &str = "PARTITION_ID";

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
    let partition_id = env::var(PARTITION_ID)?;
    let mut results = ddb_client.query()
        .set_table_name(Some(table_name.clone()))
        .set_index_name(Some(String::from("next_fire-index")))
        .set_key_condition_expression(Some(String::from("#part = :part_val AND #next_fire < :next_fire_val")))
        .set_expression_attribute_names(Some(HashMap::from([
            (String::from("#part"), String::from("part")),
            (String::from("#next_fire"), String::from("next_fire"))
        ])))
        .set_expression_attribute_values(Some(HashMap::from([
            (String::from(":part_val"), AttributeValue::S(partition_id.clone())),
            (String::from(":next_fire_val"), AttributeValue::N(next_fire_time.to_string()))
        ])))
        .into_paginator()
        .send();
    while let Some(res) = results.next().await {
        match res?.items() {
            Some(i) => for item in i {
                if let Some(v) = item.get(&String::from("topic")) {
                    match v {
                        AttributeValue::S(topic) => {
                            sns_client.publish()
                                .set_topic_arn(Some(topic.to_string()))
                                .set_message(Some(String::from("test message")))
                                .send()
                                .await?;
                        },
                        _ => println!("ignoring non-string topic {:#?}", v)
                    }
                }
                if let Some(v) = item.get(&String::from("fire_interval")) {
                    match v {
                        AttributeValue::N(interval) => {
                            if let Some(id) = item.get(&String::from("id")) {
                                match id {
                                    AttributeValue::S(idval) => {
                                        match u128::from_str(interval.as_str()) {
                                            Ok(i) => {
                                                let next = fire_time + i;
                                                ddb_client.update_item()
                                                    .set_table_name(Some(table_name.clone()))
                                                    .set_key(Some(
                                                        HashMap::from([
                                                            (String::from("part"), AttributeValue::S(partition_id.clone())),
                                                            (String::from("id"), AttributeValue::S(idval.to_string()))
                                                        ])
                                                    ))
                                                    .set_update_expression(Some(String::from("SET #fire = :i")))
                                                    .set_expression_attribute_names(Some(
                                                        HashMap::from([
                                                            (String::from("#fire"), String::from("next_fire"))
                                                        ])
                                                    ))
                                                    .set_expression_attribute_values(Some(
                                                        HashMap::from([
                                                            (String::from(":i"), AttributeValue::N(next.to_string()))
                                                        ])
                                                    ))
                                                    .send()
                                                    .await?;
                                            },
                                            Err(_) => println!("couldn't parse number: {:#?}", interval)
                                        }
                                    },
                                    _ => println!("invalid ID in item {:#?}", item)
                                }
                            } else {
                                println!("no id for item: {:#?}", item)
                            }
                        }
                        _ => println!("ignoring non-number fire_interval {:#?}", v)
                    }
                }
                println!("read item: {:#?}", item)
            },
            None => break
        }
    }
    Ok(())
}