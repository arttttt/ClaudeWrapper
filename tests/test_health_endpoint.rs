#[tokio::test]
async fn test_health_endpoint() {
    use claudewrapper::proxy::health::HealthHandler;

    let handler = HealthHandler::new();
    let resp = handler.handle().await.unwrap();

    assert_eq!(resp.status().as_u16(), 200);

    let body_bytes = resp.into_body();
    let body_str = String::from_utf8_lossy(&body_bytes);

    assert!(body_str.contains("healthy"));
    assert!(body_str.contains("claudewrapper"));
}
