use std::collections::{HashMap, HashSet};
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_kms as kms;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use lambda_runtime::{Error, LambdaEvent};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};
use tracing::info;

pub static POLICY_VERSION: &str = "2012-10-17";

// TODO: consider caching the key.
async fn get_public_key(kid: String) -> Result<Vec<u8>, Error> {
    let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
    let config = aws_config::from_env().region(region_provider).load().await;
    let kms_client = kms::Client::new(&config);
    let result = kms_client.get_public_key()
        .set_key_id(Some(kid.to_owned()))
        .send()
        .await?;
    match result.public_key() {
        Some(pk) => {
            let pk_str = STANDARD.encode(pk.to_owned().into_inner()).into_bytes();
            let mut parts: Vec<String> = vec!["-----BEGIN PUBLIC KEY-----".to_string()];
            pk_str.chunks(64).for_each(|chunk| {
               parts.push(String::from_utf8(chunk.to_vec()).unwrap());
            });
            parts.push("-----END PUBLIC KEY-----".to_string());
            println!("public_key for {} is {}", kid, parts.join("\n"));
            Ok(parts.join("\n").into())
        },
        None => Err(Error::from("missing public key"))
    }
}

pub async fn authorize(event: LambdaEvent<APIGatewayCustomAuthorizerRequest>) -> Result<APIGatewayCustomAuthorizerResponse, Error> {
    let request = event.payload;
    info!("authorize request {:#?}", request);
    if !request.authorization_token.starts_with("Bearer ") {
        return Err(Error::from("inalid authorization token"))
    }
    let header = jsonwebtoken::decode_header(&(request.authorization_token)[7..])?;
    let pubkey = get_public_key(header.kid.ok_or("no 'kid' in header")?).await?;
    let decode_key = jsonwebtoken::DecodingKey::from_ec_pem(&pubkey)?;
    let token_data = jsonwebtoken::decode::<UserClaims>(
        &(request.authorization_token)[7..],
        &decode_key,
        &jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::ES256)
    )?;
    let principal_id = token_data.claims.id;
    let tmp: Vec<&str> = request.method_arn.split(":").collect();
    let api_gateway_arn_tmp: Vec<&str> = tmp[5].split("/").collect();
    let aws_account_id = tmp[4];
    let region = tmp[3];
    let rest_api_id = api_gateway_arn_tmp[0];
    let stage = api_gateway_arn_tmp[1];
    let policy_document = APIGatewayPolicyBuilder::new(region, aws_account_id, rest_api_id, stage)
        .add_method_arn(Effect::Allow, request.method_arn)
        .build();
    let mut context_map = Map::with_capacity(2);
    context_map.insert("id".to_string(), Value::String(principal_id.to_owned()));
    context_map.insert("exp".to_string(), Value::String(token_data.claims.exp.to_string()));
    let context = Value::Object(context_map);
    println!(
        "returning APIGatewayCustomAuthorizerResponse {{ principal_id: {}, policy_document: {:#?}, context: {} }}",
        principal_id,
        policy_document,
        context
    );
    let result = APIGatewayCustomAuthorizerResponse {
        principal_id,
        policy_document,
        context,
    };
    println!("json of result: {}", serde_json::to_string(&result)?);
    Ok(result)
}

#[derive(Serialize, Deserialize)]
struct UserClaims {
    #[serde(rename = "sub")]
    id: String,
    exp: u64,
    nbf: u64
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct APIGatewayCustomAuthorizerRequest {
    #[serde(rename = "type")]
    _type: String,
    authorization_token: String,
    method_arn: String
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct APIGatewayCustomAuthorizerPolicy {
    Version: String,
    Statement: Vec<IAMPolicyStatement>
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct APIGatewayCustomAuthorizerResponse {
    principal_id: String,
    policy_document: APIGatewayCustomAuthorizerPolicy,
    context: serde_json::Value
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct IAMPolicyStatement {
    Action: Vec<String>,
    Effect: Effect,
    Resource: Vec<String>
}

#[derive(Debug)]
pub struct APIGatewayPolicyBuilder {
    region: String,
    aws_account_id: String,
    rest_api_id: String,
    stage: String,
    policy: APIGatewayCustomAuthorizerPolicy
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Method {
    #[serde(rename = "GET")]
    Get,
    #[serde(rename = "POST")]
    Post,
    #[serde(rename = "*PUT")]
    Put,
    #[serde(rename = "DELETE")]
    Delete,
    #[serde(rename = "PATCH")]
    Patch,
    #[serde(rename = "HEAD")]
    Head,
    #[serde(rename = "OPTIONS")]
    Options,
    #[serde(rename = "*")]
    All,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Effect {
    Allow,
    Deny
}

impl APIGatewayPolicyBuilder {
    pub fn new(
        region: &str,
        account_id: &str,
        api_id: &str,
        stage: &str,
    ) -> APIGatewayPolicyBuilder {
        Self {
            region: region.to_string(),
            aws_account_id: account_id.to_string(),
            rest_api_id: api_id.to_string(),
            stage: stage.to_string(),
            policy: APIGatewayCustomAuthorizerPolicy {
                Version: POLICY_VERSION.to_string(),
                Statement: vec![],
            },
        }
    }

    pub fn add_method_arn(mut self, effect: Effect, resource_arn: String) -> Self {
        let stmt = IAMPolicyStatement {
            Effect: effect,
            Action: vec!["execute-api:Invoke".to_string()],
            Resource: vec![resource_arn],
        };

        self.policy.Statement.push(stmt);
        self
    }

    pub fn add_method<T: Into<String>>(
        mut self,
        effect: Effect,
        method: Method,
        resource: T,
    ) -> Self {
        let resource_arn = format!(
            "arn:aws:execute-api:{}:{}:{}/{}/{}/{}",
            &self.region,
            &self.aws_account_id,
            &self.rest_api_id,
            &self.stage,
            serde_json::to_string(&method).unwrap(),
            resource.into().trim_start_matches("/")
        );
        self.add_method_arn(effect, resource_arn)
    }

    pub fn allow_all_methods(self) -> Self {
        self.add_method(Effect::Allow, Method::All, "*")
    }

    pub fn deny_all_methods(self) -> Self {
        self.add_method(Effect::Deny, Method::All, "*")
    }

    pub fn allow_method(self, method: Method, resource: String) -> Self {
        self.add_method(Effect::Allow, method, resource)
    }

    pub fn deny_method(self, method: Method, resource: String) -> Self {
        self.add_method(Effect::Deny, method, resource)
    }

    // Creates and executes a new child thread.
    pub fn build(self) -> APIGatewayCustomAuthorizerPolicy {
        self.policy
    }
}

#[test]
fn test_verify_sig() {
    let jwt = "eyJhbGciOiJFUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6IjNiNjQyN2M5LWNiMDktNDA4YS05YTg0LTM0ZmYzODEwMDA2ZCJ9.eyJzdWIiOiI0ZTI5NjdlZS1hMjA3LTRhMDAtOWEzMS00YTYwNDQzZDVlOTYiLCJuYmYiOjE2NzUwMTYwMDAsImV4cCI6MTY3NzMwMDkzN30.8T1SKBMgm101kHqsjxrGWZpmPi8aF8oTazVtR9pcdQ9T1DTpUQBLnZUBKJtMPzwouaZsIN9fnW3NqplN2Z6Rrw";
    //let pubkey = base64::engine::general_purpose::STANDARD.decode("MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEZqwMaZH4cJl8c2Wk55iJiyd0PwC46DdUwgNZ8Mm5gzJmX6yLWG5U02pkviJNsmH+PcB+lWNfWZ2eM2R0pdR81w==").unwrap();
    let pubkey = "-----BEGIN PUBLIC KEY-----
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEZqwMaZH4cJl8c2Wk55iJiyd0PwC4
6DdUwgNZ8Mm5gzJmX6yLWG5U02pkviJNsmH+PcB+lWNfWZ2eM2R0pdR81w==
-----END PUBLIC KEY-----";
    let verify_key = jsonwebtoken::DecodingKey::from_ec_pem(pubkey.as_bytes()).unwrap();
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::ES256);
    validation.validate_exp = false;
    let result = jsonwebtoken::decode::<UserClaims>(
        jwt,
        &verify_key,
        &validation
    ).unwrap();
}