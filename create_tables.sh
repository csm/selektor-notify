#!/bin/bash

aws --endpoint http://localhost:8000 dynamodb delete-table --table-name schedule_dev

aws --endpoint http://localhost:8000 dynamodb create-table --table-name schedule_dev --attribute-definitions AttributeName=part,AttributeType=S AttributeName=id,AttributeType=S AttributeName=next_fire,AttributeType=N AttributeName=entitlement,AttributeType=S --key-schema AttributeName=part,KeyType=HASH AttributeName=id,KeyType=RANGE --local-secondary-indexes 'IndexName=next_fire-index,KeySchema=[{AttributeName=part,KeyType=HASH},{AttributeName=next_fire,KeyType=RANGE}],Projection={ProjectionType=ALL}' 'IndexName=entitlement-index,KeySchema=[{AttributeName=part,KeyType=HASH},{AttributeName=entitlement,KeyType=RANGE}],Projection={ProjectionType=ALL}' --provisioned-throughput ReadCapacityUnits=1,WriteCapacityUnits=1

aws --endpoint http://localhost:8000 dynamodb delete-table --table-name entitlements_dev

aws --endpoint http://localhost:8000 dynamodb create-table --table-name entitlements_dev --attribute-definitions AttributeName=part,AttributeType=S AttributeName=id,AttributeType=S AttributeName=ends,AttributeType=N --key-schema AttributeName=part,KeyType=HASH AttributeName=id,KeyType=RANGE --local-secondary-indexes 'IndexName=ends-index,KeySchema=[{AttributeName=part,KeyType=HASH},{AttributeName=ends,KeyType=RANGE}],Projection={ProjectionType=ALL}' --provisioned-throughput ReadCapacityUnits=1,WriteCapacityUnits=1
