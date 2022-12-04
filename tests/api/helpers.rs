use once_cell::sync::Lazy;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use uuid::Uuid;
use wiremock::MockServer;
use zero2prod::configuration::{get_configuration, DatabaseSettings};
use zero2prod::{startup, startup::Application, telemetry};

pub(crate) struct TestApp {
    pub(crate) address: String,
    pub(crate) db_pool: PgPool,
    pub(crate) email_server: MockServer,
    pub(crate) port: u16,
}

impl TestApp {
    pub async fn post_subscriptions(&self, body: String) -> reqwest::Response {
        reqwest::Client::new()
            .post(&format!("{}/subscriptions", &self.address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("Failed to execute request")
    }
}

// Ensure that the `tracing` stack is only initialised once using `once_cell`
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
pub(crate) async fn spawn_app() -> TestApp {
    // The first time `initialize` is invoked the code in `TRACING` is executed. All other invocations
    // will instead skip execution.
    Lazy::force(&TRACING);

    // Launch a mock server to stand in for Postmark's API
    let email_server = MockServer::start().await;

    let configuration = {
        let mut c = get_configuration().expect("Failed to read configuration.");
        // Randomize the database table name for each test run, to preserve test isolation
        c.database.database_name = Uuid::new_v4().to_string();
        // Use a random OS port
        c.application.port = 0;
        c.email_client.base_url = email_server.uri();
        c
    };

    // Create and migrate the database
    configure_database(&configuration.database).await;

    let application = Application::build(configuration.clone())
        .await
        .expect("Failed to build application");

    let port = application.port();
    let address = format!("http://127.0.0.1:{}", &port);

    // launch the server as a background task
    // tokio::spawn returns a handle to the spawned future, but we have no use for it here, hence the
    // non-binding let
    let _ = tokio::spawn(application.run_until_stopped());

    TestApp {
        address,
        db_pool: startup::get_connection_pool(&configuration.database),
        email_server,
        port,
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
