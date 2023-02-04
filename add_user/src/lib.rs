use std::cmp::max;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_dynamodb as ddb;
use aws_sdk_kms as kms;
use aws_sdk_kms::model::MessageType;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use bigdecimal::{BigDecimal, ToPrimitive};
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use lambda_http::Error;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fmt::{Display, Formatter};
use std::time::{Duration, SystemTime};
use aws_sdk_dynamodb::model::AttributeAction::Add;
use jsonwebtoken::crypto::sign;

pub const XCODE_DEV_KEY: &str = "-----BEGIN PUBLIC KEY-----
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE4o5o/BwfrYZQu8bgyjF8/YtSyIRO
KKVGWQSNKVwx6YRi9VNBwOUEZ/Um/AuSK3KKPkY2SZDFbtPISk8DvKcicA==
-----END PUBLIC KEY-----";

const VERIFY_KEY: &str = "VERIFY_KEY";
const PARTITION: &str = "PARTITION";
const ENTITLEMENTS_TABLE_NAME: &str = "ENTITLEMENTS_TABLE_NAME";
const DYNAMODB_ENDPOINT: &str = "DYNAMODB_ENDPOINT";
const SIGNING_KEY_ID: &str = "SIGNING_KEY_ID";

#[derive(Debug, Serialize, Deserialize)]
pub struct AddUserRequest {
    transaction_jws: String
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddUserResponse {
    token: String
}

#[derive(Debug)]
struct AddUserError {
    pub reason: String
}

impl std::error::Error for AddUserError {}

impl Display for AddUserError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.reason)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct UserClaims {
    sub: String,
    nbf: u64,
    exp: u64
}

pub async fn add_user(request: AddUserRequest) -> Result<AddUserResponse, Error> {
    println!("add_user request: {:#?}", request);

    let verify_key_raw = env::var(VERIFY_KEY)?;
    let partition = env::var(PARTITION)?;
    let table_name = env::var(ENTITLEMENTS_TABLE_NAME)?;
    let signing_key_id = env::var(SIGNING_KEY_ID)?;

    let verify_key = STANDARD.decode(verify_key_raw)?;
    println!("verifying transaction info...");
    let user_info = verify_transaction(request.transaction_jws, verify_key)?;

    println!("verified info: {:#?}", user_info);
    let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
    let config = aws_config::from_env().region(region_provider).load().await;
    let ddb_config = match env::var(DYNAMODB_ENDPOINT) {
        Ok(endpoint) => ddb::config::Builder::from(&config).endpoint_url(endpoint).build(),
        _ => ddb::config::Builder::from(&config).build()
    };
    let ddb_client = ddb::Client::from_conf(ddb_config);
    let kms_client = kms::Client::new(&config);

    let start_millis = user_info.start_date
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_millis();
    let ends_millis = user_info.end_date
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_millis();
    let claims = UserClaims {
        sub: user_info.id.to_owned(),
        nbf: (start_millis / 1000) as u64,
        exp: (ends_millis / 1000) as u64
    };
    let encoded_claims = URL_SAFE_NO_PAD.encode(
        serde_json::to_string(&claims)?
    );
    let header = HashMap::from([
        (String::from("typ"), String::from("JWT")),
        (String::from("alg"), String::from("ES256")),
        (String::from("kid"), signing_key_id.to_owned())
    ]);
    let encoded_header = URL_SAFE_NO_PAD.encode(
        serde_json::to_string(&header)?
    );
    let pre_header = [encoded_header, encoded_claims].join(".");

    let sign_result = kms_client.sign()
        .set_signing_algorithm(Some(kms::model::SigningAlgorithmSpec::EcdsaSha256))
        .set_key_id(Some(signing_key_id))
        .set_message(Some(kms::types::Blob::new(pre_header.as_bytes())))
        .set_message_type(Some(MessageType::Raw))
        .send()
        .await?;
    let encoded_sig = URL_SAFE_NO_PAD.encode(
        transcode_to_concat(sign_result.signature().unwrap().as_ref())?
    );
    let token = [pre_header, encoded_sig].join(".");
    println!("generated new user token {}...", token.chars().into_iter().take(20).collect::<String>());

    ddb_client.put_item()
        .set_table_name(Some(table_name))
        .set_item(Some(HashMap::from([
            (String::from("part"), ddb::model::AttributeValue::S(partition)),
            (String::from("id"), ddb::model::AttributeValue::S(user_info.id)),
            (String::from("ends"), ddb::model::AttributeValue::N(
                user_info.end_date.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis().to_string()
            ))
        ])))
        .send()
        .await?;
    println!("put item into dynamodb");
    Ok(AddUserResponse{token: token})
}

#[derive(Debug)]
pub struct UserInfo {
    pub id: String,
    pub start_date: SystemTime,
    pub end_date: SystemTime
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    #[serde(rename="productId")]
    product_id: String,
    environment: String,
    quantity: i32,
    #[serde(rename="bundleId")]
    bundle_id: String,
    #[serde(rename="appAccountToken")]
    app_account_token: String,
    #[serde(rename="originalTransactionId")]
    original_transaction_id: String,
    #[serde(rename="isUpgraded")]
    is_upgraded: bool,
    #[serde(rename="expiresDate")]
    expires_date: BigDecimal,
    #[serde(rename="deviceVerificationNonce")]
    device_verification_nonce: String,
    #[serde(rename="signedDate")]
    signed_date: BigDecimal,
    #[serde(rename="subscriptionGroupIdentifier")]
    subscription_group_identifier: String,
    #[serde(rename="purchaseDate")]
    purchase_date: BigDecimal,
    #[serde(rename="type")]
    purchase_type: String,
    #[serde(rename="transactionId")]
    transaction_id: String,
    #[serde(rename="webOrderLineItemId")]
    web_order_line_item_id: String,
    #[serde(rename="deviceVerification")]
    device_verification: String,
    #[serde(rename="inAppOwnershipType")]
    in_app_ownership_type: String,
    #[serde(rename="originalPurchaseDate")]
    original_purchase_date: BigDecimal
}

pub fn verify_transaction(transaction_jws: String, pubkey: Vec<u8>) -> Result<UserInfo, Error> {
    let decoding_key = DecodingKey::from_ec_pem(&pubkey)?;
    let mut validation = Validation::new(Algorithm::ES256);
    validation.required_spec_claims = HashSet::new();
    validation.validate_exp = false;
    let token_data = jsonwebtoken::decode::<Claims>(transaction_jws.as_str(), &decoding_key, &validation)?;
    println!("decoded claims: {:?}", token_data.claims);
    let expires_date = SystemTime::UNIX_EPOCH + Duration::from_millis(token_data.claims.expires_date.round(0).as_bigint_and_exponent().0.to_u64().unwrap());
    Ok(UserInfo {
        id: token_data.claims.app_account_token,
        start_date: SystemTime::now(),
        end_date: expires_date
    })
}

fn arraycopy(src: &[u8], srcoffset: usize, dest: &mut [u8], destoffset: usize, length: usize) {
    dest[destoffset..(destoffset + length)].copy_from_slice(&src[srcoffset..(srcoffset + length)]);
}

// This is based off https://github.com/funcool/buddy-core/blob/master/src/buddy/util/ECDSA.java
pub fn transcode_to_concat(signature: &[u8]) -> Result<Vec<u8>, Error> {
    println!("transcode_to_concat: {}", STANDARD.encode(&signature));
    if signature.len() < 8 || signature[0] != 48 {
        println!("{} >= 8 || {} != 48", signature.len(), signature[0]);
        return Err(Error::from(AddUserError { reason: "invalid ECDSA signature format".to_string() }))
    }
    let mut offset: usize = 0;
    if signature[1] > 0 {
        offset = 2;
    } else if signature[1] == 0x81 {
        offset = 3;
    } else {
        println!("{} <= 0 || {} != 0x81", signature[1], signature[1]);
        return Err(Error::from(AddUserError { reason: "invalid ECDSA signature format".to_string() }))
    }

    let rlength = signature[offset + 1] as usize;
    let mut i: usize = rlength;
    while i > 0 && signature[(offset + 2 + rlength) - i] == 0 {
        i = i - 1;
    }

    let slength = signature[offset + 2 + rlength + 1] as usize;
    let mut j: usize = slength;
    while j > 0 && signature[(offset + 2 + rlength + 2 + slength) - j] == 0 {
        j = j - 1;
    }

    let rawlen = max(max(i, j), 32);

    if signature[offset - 1] as usize != signature.len() - offset ||
        signature[offset - 1] as usize != 2 + rlength + 2 + slength ||
        signature[offset] != 2 ||
        signature[offset + 2 + rlength] != 2
    {
        println!("{} != {} - {} || {} != 2 + {} + 2 + {} || {} != 2 || {} != 2",
            signature[offset - 1], signature.len(), offset,
            signature[offset - 1], rlength, slength,
            signature[offset],
            signature[offset + 2 + rlength]
        );
        return Err(Error::from(AddUserError { reason: "Invalid ECDSA signature format".to_string()}))
    }
    let mut output: Vec<u8> = vec![0; 64];
    arraycopy(&signature, (offset + 2 + rlength) - i, &mut output, rawlen - i, i);
    arraycopy(&signature, (offset + 2 + rlength + 2 + slength) - j, &mut output, 2 * rawlen - j, j);
    Ok(output)
}

#[cfg(test)]
const TEST_JWS: &str = "eyJraWQiOiJBcHBsZV9YY29kZV9LZXkiLCJ4NWMiOlsiTUlJQnpEQ0NBWEdnQXdJQkFnSUJBVEFLQmdncWhrak9QUVFEQWpCSU1TSXdJQVlEVlFRREV4bFRkRzl5WlV0cGRDQlVaWE4wYVc1bklHbHVJRmhqYjJSbE1TSXdJQVlEVlFRS0V4bFRkRzl5WlV0cGRDQlVaWE4wYVc1bklHbHVJRmhqYjJSbE1CNFhEVEl6TURFeU5UQTBOVFV6TjFvWERUSTBNREV5TlRBME5UVXpOMW93U0RFaU1DQUdBMVVFQXhNWlUzUnZjbVZMYVhRZ1ZHVnpkR2x1WnlCcGJpQllZMjlrWlRFaU1DQUdBMVVFQ2hNWlUzUnZjbVZMYVhRZ1ZHVnpkR2x1WnlCcGJpQllZMjlrWlRCWk1CTUdCeXFHU000OUFnRUdDQ3FHU000OUF3RUhBMElBQk9LT2FQd2NINjJHVUx2RzRNb3hmUDJMVXNpRVRpaWxSbGtFalNsY01lbUVZdlZUUWNEbEJHZjFKdndMa2l0eWlqNUdOa21ReFc3VHlFcFBBN3luSW5DalREQktNQklHQTFVZEV3RUJcL3dRSU1BWUJBZjhDQVFBd0pBWURWUjBSQkIwd0c0RVpVM1J2Y21WTGFYUWdWR1Z6ZEdsdVp5QnBiaUJZWTI5a1pUQU9CZ05WSFE4QkFmOEVCQU1DQjRBd0NnWUlLb1pJemowRUF3SURTUUF3UmdJaEFQUHdMSlp5bUZLR2xCK2RQdHUwOFlDZnIxXC9rOXVKY21hZkNBM3hINzNSMEFpRUEyckRkQVRZUUZRRmVveW0rbmpGcGRFMEtBN3B0MkE2Z245dm1pRVFnaFwvVT0iXSwidHlwIjoiSldUIiwiYWxnIjoiRVMyNTYifQ.eyJwcm9kdWN0SWQiOiJvcmcubWV0YXN0YXRpYy5zZWxla3Rvci5zdWJzY3JpcHRpb24ubW9udGhseSIsImVudmlyb25tZW50IjoiWGNvZGUiLCJxdWFudGl0eSI6MSwiYnVuZGxlSWQiOiJvcmcubWV0YXN0YXRpYy5TZWxla3RvciIsImFwcEFjY291bnRUb2tlbiI6IjRlMjk2N2VlLWEyMDctNGEwMC05YTMxLTRhNjA0NDNkNWU5NiIsIm9yaWdpbmFsVHJhbnNhY3Rpb25JZCI6IjAiLCJpc1VwZ3JhZGVkIjpmYWxzZSwiZXhwaXJlc0RhdGUiOjE2NzczMDA5MzcwNTAuMjk3MSwiZGV2aWNlVmVyaWZpY2F0aW9uTm9uY2UiOiI4YjUzMGFlNS0wYmIwLTQ2ZjQtYmJmZi0wOTc5MDM2MTg2MDkiLCJzaWduZWREYXRlIjoxNjc0NjIyNTM3MDc1LjkyMzgsInN1YnNjcmlwdGlvbkdyb3VwSWRlbnRpZmllciI6IjIxMTAwMjgyIiwicHVyY2hhc2VEYXRlIjoxNjc0NjIyNTM3MDUwLjI5NzEsInR5cGUiOiJBdXRvLVJlbmV3YWJsZSBTdWJzY3JpcHRpb24iLCJ0cmFuc2FjdGlvbklkIjoiMCIsIndlYk9yZGVyTGluZUl0ZW1JZCI6IjAiLCJkZXZpY2VWZXJpZmljYXRpb24iOiJoNTdyeFQyNlVpMzdwTUdpc3ZOR2xrV2E4U05jWDlYejJOMkdaRXlZZ2ZXVExObE5NTHNcL2xVb0ZrbGxUbjlmUiIsImluQXBwT3duZXJzaGlwVHlwZSI6IlBVUkNIQVNFRCIsIm9yaWdpbmFsUHVyY2hhc2VEYXRlIjoxNjc0NjIyNTM3MDUwLjI5NzF9.QrSL8WI2nVXq2dq3rvWGF1Ga187SDX9MrE2i6LI0gsP6KFB84rgyxfntkFxQS_3314AfxMdGnCyHNfvpVav5qQ";

#[test]
fn test_verify() {
    let result = verify_transaction(String::from(TEST_JWS), XCODE_DEV_KEY.as_bytes().to_vec()).unwrap();
    assert_eq!(result.id, String::from("4e2967ee-a207-4a00-9a31-4a60443d5e96"));
    assert_eq!(result.end_date.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis(), 1677300937050);
}

#[test]
fn test_gen_key() {
    let genkey = "eyJraWQiOiIzYjY0MjdjOS1jYjA5LTQwOGEtOWE4NC0zNGZmMzgxMDAwNmQiLCJ0eXAiOiJKV1QiLCJhbGciOiJFUzI1NiJ9.eyJzdWIiOiI0ZTI5NjdlZS1hMjA3LTRhMDAtOWEzMS00YTYwNDQzZDVlOTYiLCJuYmYiOjE2NzQ5MTk0MDIsImV4cCI6MTY3NzMwMDkzN30.MEYCIQDFSUof9g5eIvO_hQR0ZknsAKyFAAvJyMkZff9_D5VM0QIhANR9PRoeyWNymaa05PIS9kdGe26MJRf10iiiGev1cQ0U";
    let pubkey = "-----BEGIN PUBLIC KEY-----
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEZqwMaZH4cJl8c2Wk55iJiyd0PwC4
6DdUwgNZ8Mm5gzJmX6yLWG5U02pkviJNsmH+PcB+lWNfWZ2eM2R0pdR81w==
-----END PUBLIC KEY-----";
    let decoding_key = DecodingKey::from_ec_pem(pubkey.as_ref()).unwrap();
    let result = jsonwebtoken::decode::<UserClaims>(genkey, &decoding_key, &Validation::new(Algorithm::ES256)).unwrap();
}

#[test]
fn test_transcode_to_concat() {
    let sig = URL_SAFE_NO_PAD.decode("MEYCIQDFSUof9g5eIvO_hQR0ZknsAKyFAAvJyMkZff9_D5VM0QIhANR9PRoeyWNymaa05PIS9kdGe26MJRf10iiiGev1cQ0U").unwrap();
    let sig2 = transcode_to_concat(sig.as_ref()).unwrap();
    println!("transcoded: {}", STANDARD.encode(sig2));
}