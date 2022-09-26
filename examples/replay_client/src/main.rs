use std::time::Duration;

use smoo::{guid::Guid, server::Server, settings::Settings, test::mockclient::MockClient};
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

    let mock_client = MockClient::simple_connect(bind_addr).await;
    let target_guid = [
        126, 128, 87, 52, 186, 45, 0, 16, 175, 237, 95, 234, 197, 104, 21, 75,
    ];

    let cli_task = tokio::task::spawn(mock_client.replay_player(target_guid.into()));
    let (_, _) = tokio::join!(serv_task, cli_task);
}
