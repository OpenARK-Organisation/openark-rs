use openark_rs::{OpenArkServiceProvider, OpenArkUser};
use std::time::Duration;
use testcontainers::{
    GenericImage,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};

async fn start_relay() -> (testcontainers::ContainerAsync<GenericImage>, String) {
    let container = GenericImage::new("strfry-strfry", "latest")
        .with_exposed_port(7777.tcp())
        .with_wait_for(WaitFor::message_on_stderr("Started websocket server"))
        .start()
        .await
        .expect("Failed to start strfry container");

    let host_port = container
        .get_host_port_ipv4(7777)
        .await
        .expect("Failed to map strfry port");

    (container, format!("ws://localhost:{host_port}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn provider_sends_hello_user_replies_how_are_you_today() {
    let (_relay_container, relay_url) = start_relay().await;

    let provider = OpenArkServiceProvider::connect(&relay_url)
        .await
        .expect("provider should connect");
    let user = OpenArkUser::connect(&relay_url)
        .await
        .expect("user should connect");

    tokio::time::sleep(Duration::from_secs(1)).await;

    let user_flow = user.reply_when_provider_says_hello(provider.pubkey, Duration::from_secs(10));
    let provider_flow = async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        provider.send_hello().await.expect("provider should send hello");
        provider
            .wait_for_user_reply(user.pubkey, Duration::from_secs(10))
            .await
            .expect("provider should wait for user reply")
    };

    let (user_replied, provider_received_reply) = tokio::join!(user_flow, provider_flow);
    assert!(user_replied.expect("user flow should run"));
    assert!(provider_received_reply);
}
