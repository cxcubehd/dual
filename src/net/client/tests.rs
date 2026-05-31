use super::*;

#[test]
fn test_client_creation() {
    let config = ClientConfig::default();
    let client = NetworkClient::new(config);
    assert!(client.is_ok());

    let client = client.unwrap();
    assert_eq!(client.state(), ConnectionState::Disconnected);
}
