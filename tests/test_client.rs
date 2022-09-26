use std::time::Duration;

use smoo::{
    net::{Packet, PacketData},
    server::Server,
    settings::Settings,
    test::mockclient::MockClient,
    types::Vector3,
};
use tokio::time::{sleep, timeout};

const DEFAULT_TIMEOUT_MS: u64 = 100;

async fn create_server() -> Server {
    let mut settings = Settings::default();
    settings.server.address = "127.0.0.1".parse().unwrap();
    settings.server.port = 0;

    let mut server = Server::build_server(settings);
    server.listener.udp_port_addrs = None;
    server.bind_addresses().await.unwrap();
    server
}

#[test_log::test(tokio::test)]
async fn test_client_join() {
    let server = create_server().await;

    let addr = server.get_bind_addr();
    let _serv_task = tokio::task::spawn(server.spawn_minimal_server());

    sleep(Duration::from_secs(1)).await;
    let _mock_client_1 = MockClient::simple_connect(addr).await;
}

#[test_log::test(tokio::test)]
async fn test_two_client_handshake() {
    let server = create_server().await;
    let addr = server.get_bind_addr();
    let _serv_task = tokio::task::spawn(server.spawn_minimal_server());
    sleep(Duration::from_secs(1)).await;

    let mut mock1 =
        MockClient::connect(addr, "1000000000-2000-3000-4000-5000000000", "Mock1").await;
    let mut mock2 =
        MockClient::connect(addr, "1020304050-0000-0000-0000-0000000000", "Mock2").await;
    tracing::info!("Got mocks");
    // Recv the udp init packets
    let _udp_init = timeout(Duration::from_millis(100), mock1.get_packet())
        .await
        .expect("Udp handshake packet timed out");
    let _udp_init = timeout(Duration::from_millis(100), mock2.get_packet())
        .await
        .expect("Udp handshake packet timed out");

    // Recv the connect packets
    let join_1 = timeout(Duration::from_millis(100), mock1.get_packet())
        .await
        .expect("Connect handshake packet timed out");
    let join_2 = timeout(Duration::from_millis(100), mock2.get_packet())
        .await
        .expect("Connect handshake packet timed out");

    // Verify the connect packets
    match join_1.data {
        PacketData::Connect { client_name, .. } => assert_eq!(client_name, "Mock2"),
        _ => panic!("Join 1 has wrong packet type"),
    }

    match join_2.data {
        PacketData::Connect { client_name, .. } => assert_eq!(client_name, "Mock1"),
        _ => panic!("Join 2 has wrong packet type"),
    }

    // First client waits till second sends costume normally
    // So first client left out of recving

    // let _costume_1 = mock1.get_packet().await;
    let _costume_2 = timeout(
        Duration::from_millis(DEFAULT_TIMEOUT_MS),
        mock2.get_packet(),
    )
    .await
    .expect("Costume handshake packet timed out");
}

async fn finish_mock_handshake(mock1: &mut MockClient, mock2: &mut MockClient) {
    // Receive server udp init packets
    tracing::debug!("Finishing udp handshake");
    let _udp_init = timeout(
        Duration::from_millis(DEFAULT_TIMEOUT_MS),
        mock1.get_packet(),
    )
    .await
    .expect("Udp handshake packet timed out");
    let _udp_init = timeout(
        Duration::from_millis(DEFAULT_TIMEOUT_MS),
        mock2.get_packet(),
    )
    .await
    .expect("Udp handshake packet timed out");

    // Receive other players connect packets
    tracing::debug!("Finishing join handshake");
    let _join_1 = timeout(
        Duration::from_millis(DEFAULT_TIMEOUT_MS),
        mock1.get_packet(),
    )
    .await
    .expect("Connect handshake packet timed out");
    let _join_2 = timeout(
        Duration::from_millis(DEFAULT_TIMEOUT_MS),
        mock2.get_packet(),
    )
    .await
    .expect("Connect handshake packet timed out");

    // Receive other players custume packets
    tracing::debug!("Finishing costume handshake");
    // let _costume_1 = mock1.get_packet().await;
    let _costume_2 = timeout(
        Duration::from_millis(DEFAULT_TIMEOUT_MS),
        mock2.get_packet(),
    )
    .await
    .expect("Costume handshake packet timed out");
}

#[test_log::test(tokio::test)]
async fn test_movement() {
    let server = create_server().await;
    let addr = server.get_bind_addr();
    let _serv_task = tokio::task::spawn(server.spawn_minimal_server());
    sleep(Duration::from_secs(1)).await;

    let m1_guid = "1000000000-2000-3000-4000-5000000000".try_into().unwrap();
    let m2_guid = "1020304050-0000-0000-0000-0000000000".try_into().unwrap();
    let mut mock_client_1 = MockClient::connect(addr, m1_guid, "Mock1").await;
    sleep(Duration::from_millis(10)).await;
    let mut mock_client_2 = MockClient::connect(addr, m2_guid, "Mock2").await;
    tracing::info!("Got mocks");

    tracing::info!("Finishing handshake");
    finish_mock_handshake(&mut mock_client_1, &mut mock_client_2).await;

    let packet_data = PacketData::Player {
        pos: Vector3::x(),
        rot: Default::default(),
        animation_blend_weights: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
        act: 10,
        sub_act: 0,
    };

    tracing::info!("Sending for first movement packet");
    let packet = Packet::new(m1_guid, packet_data);
    mock_client_1.send_packet(&packet).await;
    let recv_packet = mock_client_2.get_packet().await;
    assert_eq!(packet, recv_packet);

    let packet_data = PacketData::Player {
        pos: Vector3::y(),
        rot: Default::default(),
        animation_blend_weights: [6.0, 5.0, 4.0, 3.0, 2.0, 1.0],
        act: 10,
        sub_act: 0,
    };

    tracing::info!("Sending for second movement packet");
    let packet = Packet::new(m2_guid, packet_data);
    mock_client_2.send_packet(&packet).await;
    let recv_packet = mock_client_1.get_packet().await;
    assert_eq!(packet, recv_packet);
}
