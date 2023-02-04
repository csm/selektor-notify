use std::cmp::Ordering;
use std::collections::HashMap;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_dynamodb as ddb;
use aws_sdk_dynamodb::model::AttributeValue;
use lambda_http::Error;
use std::env;
use std::str::FromStr;
use lambda_http::aws_lambda_events::serde::{Deserialize, Serialize};
use tracing::info;
use tokio_stream::StreamExt;

const PARTITION_ID: &str = "PARTITION_ID";
const TABLE_NAME: &str = "TABLE_NAME";
const DYNAMODB_ENDPOINT: &str = "DYNAMODB_ENDPOINT";

#[derive(Serialize, Deserialize, Copy, Clone, Debug)]
pub struct ScheduleEntry {
    last_fire: u64,
    fire_interval: u64
}

impl PartialEq<ScheduleEntry> for ScheduleEntry {
    fn eq(&self, other: &Self) -> bool {
        return self.last_fire == other.last_fire && self.fire_interval == other.fire_interval
    }
}

impl Eq for ScheduleEntry {}

impl PartialOrd<ScheduleEntry> for ScheduleEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        return Some(
            if self.last_fire < other.last_fire {
                Ordering::Less
            } else if self.last_fire > other.last_fire {
                Ordering::Greater
            } else {
                if self.fire_interval < other.fire_interval {
                    Ordering::Less
                } else if self.fire_interval > other.fire_interval {
                    Ordering::Greater
                } else {
                    Ordering::Equal
                }
            }
        )
    }
}

impl Ord for ScheduleEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateScheduleRequest {
    entries: Vec<ScheduleEntry>
}

fn decode_schedule(item: &HashMap<String, AttributeValue>) -> Option<ScheduleEntry> {
    if let Some(AttributeValue::N(next_fire_n)) = item.get("next_fire") {
        if let Some(AttributeValue::N(fire_interval_n)) = item.get("fire_interval") {
            if let Ok(next_fire) = u64::from_str(next_fire_n) {
                if let Ok(fire_interval) = u64::from_str(fire_interval_n) {
                    Some(ScheduleEntry { last_fire: next_fire - fire_interval, fire_interval })
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    }
}

pub async fn update_schedule(principal: &String, request: &UpdateScheduleRequest) -> Result<(), Error> {
    let partition_id = env::var(PARTITION_ID)?;
    let table_name = env::var(TABLE_NAME)?;

    let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
    let config = aws_config::from_env().region(region_provider).load().await;
    let ddb_config = match env::var(DYNAMODB_ENDPOINT) {
        Ok(endpoint) => ddb::config::Builder::from(&config).endpoint_url(endpoint).build(),
        _ => ddb::config::Builder::from(&config).build()
    };
    let ddb_client = ddb::Client::from_conf(ddb_config);

    // Fetch the current schedules.
    let mut results = ddb_client.query()
        .table_name(table_name.to_owned())
        .index_name("part-entitlement-index")
        .key_condition_expression("#part = :part AND #ent = :ent")
        .expression_attribute_names("#part", "part")
        .expression_attribute_names("#ent", "entitlement")
        .expression_attribute_values(":part", AttributeValue::S(partition_id.to_owned()))
        .expression_attribute_values(":ent", AttributeValue::S(principal.to_string()))
        .into_paginator()
        .send();
    let mut existing_schedules: Vec<ScheduleEntry> = Vec::new();
    let mut existing_ids: Vec<String> = Vec::new();
    while let Some(res) = results.next().await {
        match res?.items() {
            Some(items) => for item in items {
                if let Some(sched) = decode_schedule(item) {
                    if let Some(AttributeValue::S(id)) = item.get("id") {
                        existing_ids.push(id.to_string());
                    }
                    existing_schedules.push(sched)
                }
            }
            None => break
        }
    }
    existing_schedules.sort();

    let mut new_sched = request.entries.to_vec();
    new_sched.sort();

    if existing_schedules == new_sched {
        info!("schedules are equal, skipping update");
        return Ok(())
    }

    for id in existing_ids {
        ddb_client.delete_item()
            .table_name(table_name.to_owned())
            .key("part", AttributeValue::S(partition_id.to_owned()))
            .key("id", AttributeValue::S(id.to_owned()))
            .send()
            .await?;
    }

    for sched in new_sched {
        ddb_client.put_item()
            .table_name(table_name.to_owned())
            .item("part", AttributeValue::S(partition_id.to_owned()))
            .item("id", AttributeValue::S(uuid::Uuid::new_v4().to_string()))
            .item("entitlement", AttributeValue::S(principal.to_owned()))
            .item("next_fire", AttributeValue::N((sched.last_fire + sched.fire_interval).to_string()))
            .item("fire_interval", AttributeValue::N(sched.fire_interval.to_string()))
            .send()
            .await?;
    }

    Ok(())
}