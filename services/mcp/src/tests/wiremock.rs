use wiremock::MockServer;

//we can use this object on another files
//TODO: ill use this for testing hivemind behaviors on tests/hivemind.rs
pub async fn spawn_server() -> MockServer {
    MockServer::start().await
}
