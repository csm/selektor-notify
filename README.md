# selektor-notify

AWS Lambda functions for managing Selektor push notifications.

## add_user

API Gateway endpoint.

Called by Apple's App Store on in-app subscription purchase events.

- Verifies payload, and adds info to dynamodb.

## register_push

API Gateway endpoint.

Signs up to get push notifications given a schedule.

- Verifies caller has entitlements in dynamodb.
- Installs SNS info for sending notifications.
- Installs initial schedule.
- Generates app token for further updates from the app.

## run_notify

Lambda invoked on schedule from CloudWatch events, every 5 minutes.

- Scan dynamodb for schedules that need to be run.
- Post SNS notifications for each schedule.
- 

## update_schedule

API Gateway endpoint.

Alters an existing user's schedule in dynamodb.

## dynamodb tables

### entitlements

|Name|Type|Comment|
|----|----|-|
|id|string|Unique ID of the entitlement.|
|ends|timestamp|When the subscription ends.|

### schedules

|Name|Type|Comments|
|----|----|--------|
|id|string|Unique ID.|
|next_fire|timestamp|When the next fire date is.|
|entitlement|string|The ID of the associated entitlement.|