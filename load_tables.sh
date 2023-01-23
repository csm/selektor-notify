#!/bin/bash

# shellcheck disable=SC2006
DISTANT_PAST=$(((`date +%s` - (365 * 24 * 60 * 60)) * 1000))
#DISTANT_FUTURE=$((`date +%s` + (365 * 24 * 60 * 60)))

PAST_ID=$(uuidgen)
aws --endpoint http://localhost:8000 dynamodb put-item --table-name entitlements_dev --item "{\"part\":{\"S\":\"default\"},\"id\":{\"S\":\"$PAST_ID\"},\"ends\":{\"N\":\"$DISTANT_PAST\"}}"
aws --endpoint http://localhost:8000 dynamodb put-item --table-name schedule_dev --item "{\"part\":{\"S\":\"default\"},\"id\":{\"S\":\"$(uuidgen)\"},\"next_fire\":{\"N\":\"0\"},\"fire_interval\":{\"N\":\"12\"},\"entitlement\":{\"S\":\"$PAST_ID\"}}"