use once_cell::sync::Lazy;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use std::net::TcpListener;
use uuid::Uuid;
use zero2prod::configuration::{get_configuration, DatabaseSettings};
use zero2prod::{email_client::EmailClient, telemetry};

struct TestApp {
    address: String,
    db_pool: PgPool,
}

/// `tokio::test` is the testing equivalent of `tokio::main`. It also spares you from having to specify
/// the `#[test]` attribute.
/// You can check what code gets generated using `cargo expand --test health_check` ( <- name of the test file)
#[tokio::test]
async fn health_check_works() {
    // Arrange
    let app = spawn_app().await;
    // We need to bring in `reqwest` to perform HTTP requests against our application.
    let client = reqwest::Client::new();

    // Act
    let response = client
        .get(format!("{}/health_check", &app.address))
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
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    // Act
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    let response = client
        .post(&format!("{}/subscriptions", &app.address))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .expect("Failed to execute request");

    // Assert
    assert_eq!(200, response.status().as_u16());

    let saved = sqlx::query!("SELECT email, name FROM subscriptions",)
        .fetch_one(&app.db_pool)
        .await
        .expect("Failed to fetch saved subscription.");

    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
}

#[tokio::test]
async fn subscribe_returns_a_400_when_fields_are_present_but_invalid() {
    // Arrange
    let app = spawn_app().await;
    let client = reqwest::Client::new();
    let url = format!("{}/subscriptions", &app.address);
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

#[tokio::test]
async fn subscribe_returns_a_200_when_fields_are_present_but_empty() {
    // Arrange
    let app = spawn_app().await;
    let client = reqwest::Client::new();
    let test_cases = vec![
        ("name=&email=coolkrishna31%40gmail.com", "empty name"),
        ("name=Ursula&email=", "empty email"),
        ("name=Ursula&email=definitely-not-an-email", "invalid email"),
    ];

    for (body, description) in test_cases {
        //Act
        let response = client
            .post(&format!("{}/subscriptions", &app.address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("Failed to execute request");

        // Assert
        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not return a 200 OK when the payload was {}.",
            description
        );
    }
}
/// Ensure that the `tracing` stack is only initialised once using `once_cell`
static TRACING: Lazy<()> = Lazy::new(|| {
    let default_filter_level = "info".to_string();
    let subscriber_name = "test".to_string();

    // We cannot assign the output of `get_subscriber` to a variable based on the value TEST_LOG because
    // the sink is part of the type returned by `get_subscriber`, therefore they are not the same type.
    // We could work around it, but this is the most straight-forward way of moving forward.
    if std::env::var("TEST_LOG").is_ok() {
        let subscriber =
            telemetry::get_subscriber(subscriber_name, default_filter_level, std::io::stdout);
        telemetry::init_subscriber(subscriber);
    } else {
        let subscriber =
            telemetry::get_subscriber(subscriber_name, default_filter_level, std::io::sink);
        telemetry::init_subscriber(subscriber);
    }
});

/// No `.await` call, therefore no need for `spawn_app` to be async now. We are also running tests, so
/// it is not worth it to propagate errors: if we fail to perform the required setup we can just panic
/// and crash all the things.
async fn spawn_app() -> TestApp {
    // The first time `initialize` is invoked the code in `TRACING` is executed. All other invocations
    // will instead skip execution.
    Lazy::force(&TRACING);

    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");
    //Retrieve the port assigned to us by the OS
    let port = listener.local_addr().unwrap().port();
    let address = format!("http://127.0.0.1:{port}");

    let mut configuration = get_configuration().expect("Failed to read configuration.");
    // Randomize the database table name for each test run, to preserve test isolation
    configuration.database.database_name = Uuid::new_v4().to_string();

    let connection_pool = configure_database(&configuration.database).await;

    //Build a new email client
    let sender_email = configuration
        .email_client
        .sender()
        .expect("Invalid sender email address.");
    let email_client = EmailClient::new(
        configuration.email_client.base_url,
        sender_email,
        configuration.email_client.authorization_token,
    )
    .expect("Unable to build email client");

    let server = zero2prod::startup::run(listener, connection_pool.clone(), email_client)
        .expect("Failed to bind address");
    // launch the server as a background task
    // tokio::spawn returns a handle to the spawned future, but we have no use for it here, hence the
    // non-binding let
    let _ = tokio::spawn(server);

    TestApp {
        address,
        db_pool: connection_pool,
    }
}

/// The database is a gigantic global variable: all our tests are interacting with it and whatever
/// they leave behind will be available to other tests in the suite as well as to the following test
/// runs.
///
/// We really don't want to have *any* kind of interaction between our tests: it makes our test runs
/// non-deterministic and it leads down the line to spurious test failures that are extremely tricky
/// to hunt down and fix.
///
/// There are two well known techniques to ensure test isolation when interacting with a relational
/// database in a test:
/// * wrap the whole test in a SQL transaction and rollback at the end of it;
/// * spin up a brand-new logical database for each integration test.
///
/// The first is clever and will generally be faster: rolling back a SQL transaction takes less time
/// than spinning up a new logical database. It works quite well when writing unit tests for your
/// queries but it is tricky to pull off in an integration test like ours: our application will borrow
/// a `PgConnection` from a `PgPool` and we have no way to "capture" that connection in a SQL transaction
/// context.
/// This leads us to the second option: potentially slower, yet much easier to implement. Before each
/// test run, we want to:
/// * create a new logical database with a unique name;
/// * run database migrations on it.
///
/// The best place to do this is in spawn_app, before launching our actix-web test application.
async fn configure_database(config: &DatabaseSettings) -> PgPool {
    let mut connection = PgConnection::connect_with(&config.without_db())
        .await
        .expect("Failed to connect to Postgres");

    connection
        .execute(format!(r#"CREATE DATABASE "{}"; "#, config.database_name).as_str())
        .await
        .expect("Failed to create database.");

    //Migrate database
    let connection_pool = PgPool::connect_with(config.with_db())
        .await
        .expect("Failed to connect to Postgres.");

    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate the database");

    connection_pool
}
