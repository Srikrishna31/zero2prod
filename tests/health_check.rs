use sqlx::{Connection, PgConnection};
use std::net::TcpListener;
use zero2prod::configuration::get_configuration;

/// `tokio::test` is the testing equivalent of `tokio::main`. It also spares you from having to specify
/// the `#[test]` attribute.
/// You can check what code gets generated using `cargo expand --test health_check` ( <- name of the test file)
#[tokio::test]
async fn health_check_works() {
    // Arrange
    let address = spawn_app();
    // We need to bring in `reqwest` to perform HTTP requests against our application.
    let client = reqwest::Client::new();

    // Act
    let response = client
        .get(format!("{}/health_check", &address))
        .send()
        .await
        .expect("Failed to execute request");

    // Assert
    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
}

#[tokio::test]
async fn subscribe_returns_a_200_for_valid_form_data() {
    // Arrange
    let app_address = spawn_app();
    let configuration = get_configuration().expect("Failed to read configuration");
    let connection_string = configuration.database.connection_string();

    // The `Connection` trait MUST be in scope for us to invoke `PgConnection::connect` - it is not
    // an ineherent method of the struct!
    let _connection = PgConnection::connect(&connection_string)
        .await
        .expect("Failed to connect to Postgres.");
    let client = reqwest::Client::new();

    // Act
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    let response = client
        .post(&format!("{app_address}/subscriptions"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .expect("Failed to execute request");

    // Assert
    assert_eq!(200, response.status().as_u16());
}

#[tokio::test]
async fn subscribe_returns_a_400_when_data_is_missing() {
    // Arrange
    let app_address = spawn_app();
    let client = reqwest::Client::new();
    let url = format!("{}/subscriptions", &app_address);
    // Act
    let test_cases = vec![
        ("name=le%20guin", "missing the mail"),
        ("email=ursula_le_%40gmail.com", "missing the name"),
        ("", "missing both name and email"),
    ];

    for (invalid_body, error_message) in test_cases {
        // Act
        let response = client
            .post(&url)
            .header("Content-type", "application/x-www-form-urlencoded")
            .body(invalid_body)
            .send()
            .await
            .expect("Failed to execute request.");

        // Assert
        assert_eq!(
            400,
            response.status().as_u16(),
            // Additional customised error message on test failure
            "The API did not fail with 400 Bad Request when the payload was {}.",
            error_message
        )
    }
}
/// No .await call, therefore no need for `spawn_app` to be async now. We are also running tests, so
/// it is not worth it to propagate errors: if we fail to perform the required setup we can just panic
/// and crash all the things.
fn spawn_app() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");
    //Retrieve the port assigned to us by the OS
    let port = listener.local_addr().unwrap().port();

    let server = zero2prod::startup::run(listener).expect("Failed to bind address");
    // launch the server as a background task
    // tokio::spawn returns a handle to the spawned future, but we have no use for it here, hence the
    // non-binding let
    let _ = tokio::spawn(server);

    // Return the application address to the caller!
    format!("http://127.0.0.1:{port}")
}
