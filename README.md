# selektor-notify

AWS Lambda functions for managing Selektor push notifications.

This was a part of a now-abandoned personal project to make an iPhone
app for monitoring websites for changes. Consider this just a learning
experience for me on how to use Rust as an AWS lambda.

The idea here was to periodically send the [iOS app](https://github.com/csm/selektor)
background push notifications to inform it to update the URLs it tracks.
This was because background updates were infrequent or not firing at all,
but this turned out not to work either. The flow here was intended to be:

* The user subscribes to the app, and the app sends the signed receipt to
  the cloud.
* After verifying the receipt, issue a new JWT so the app can authenticate
  itself.
* The app then uploads the schedule it needs notifications for.

Then, periodically:

* Scan dynamodb for notifications that need to be sent, send them with SNS,
  and then update the schedule entry. This would run every 5 minutes.
* Scan dynamodb for expired subscriptions, and delete the entries in the
  schedule table. This would run daily.

Basic costs for this (not at scale) was $1 per month, due entirely to the
KMS key.

Rust is fun to write AWS lambdas in! And they're super fast, which translates
to being cheap to run.

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
- Update schedules with next fire date.

## update_schedule

API Gateway endpoint.

Alters an existing user's schedule in dynamodb.

## dynamodb tables

### entitlements

| Name | Type   | Comment                       |
|------|--------|-------------------------------|
| part | string | Partition ID.                 |
| id   | string | Unique ID of the entitlement. |
| ends | number | When the subscription ends.   |

#### Secondary Indexes

* `ends`

### schedules

| Name          | Type   | Comments                                                                   |
|---------------|--------|----------------------------------------------------------------------------|
| part          | string | Partition ID.                                                              |
| id            | string | Unique ID.                                                                 |
| next_fire     | number | When the next fire date is (units are 5 minute intervals since the epoch). |
| topic         | string | Topic ARN for posting SNS events.                                          |
| entitlement   | string | The ID of the associated entitlement.                                      |
| fire_interval | number | The interval between fires.                                                |

#### Secondary Indexes

* `next_fire` —
* `entitlement` —