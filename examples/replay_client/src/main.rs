use std::time::Duration;

use smoo::{guid::Guid, server::Server, settings::Settings, test::mockclient::MockClient};
use tokio::task::futures;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_panic(info);
        std::process::exit(1);
    }));

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let mut settings = Settings::default();
    settings.server.port = 1029;

    let server = Server::build_server(settings);
    let bind_addr = server.get_bind_addr();
    let serv_task = tokio::task::spawn(server.spawn_minimal_server());
    tokio::time::sleep(Duration::from_secs(1)).await;

    let target_guid: Guid = "aa466c1095-9557-59ea-fe67-f6da9befa7".try_into().unwrap();

    let mut client_tasks = vec![];
    for i in 0..3 {
        let mut new_guid = target_guid.clone().id;
        new_guid[0] = i;

        let mock_client = MockClient::connect(bind_addr, new_guid, format!("Mock{}", i)).await;
        let client_task = tokio::task::spawn(mock_client.replay_player(target_guid.clone().into()));
        client_tasks.push(client_task);
    }

    let _ = futures_util::future::join_all(client_tasks);
    let _ = tokio::join!(serv_task);
}
