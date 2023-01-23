use run_notify;
use aws_lambda_events::event::cloudwatch_events::CloudWatchEvent;
use lambda_runtime::{Context, LambdaEvent};

#[test]
fn test_handler() {
    let future = run_notify::function_handler(LambdaEvent {
        payload: CloudWatchEvent {
            version: None,
            id: None,
            detail_type: None,
            source: None,
            account_id: None,
            time: Default::default(),
            region: None,
            resources: vec![],
            detail: None,
        },
        context: Default::default()
    });
    let res = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future);
    println!("handler returned {:#?}", res)
}