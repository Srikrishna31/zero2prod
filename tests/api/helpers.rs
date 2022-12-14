use argon2::password_hash::SaltString;
use argon2::{Algorithm, Argon2, Params, PasswordHasher, Version};
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
    pub(crate) test_user: TestUser,
}

/// Confirmation links embedded in the request to the email API.
pub(crate) struct ConfirmationLinks {
    pub(crate) html: reqwest::Url,
    pub(crate) plain_text: reqwest::Url,
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

    /// Extract the confirmation links embedded in the request to the email API.
    pub fn get_confirmation_links(&self, email_request: &wiremock::Request) -> ConfirmationLinks {
        let body: serde_json::Value = serde_json::from_slice(&email_request.body).unwrap();

        // Extract the link from one of the request fields.
        let get_link = |s: &str| {
            let links: Vec<_> = linkify::LinkFinder::new()
                .links(s)
                .filter(|l| *l.kind() == linkify::LinkKind::Url)
                .collect();

            assert_eq!(links.len(), 1);

            let raw_link = links[0].as_str().to_owned();
            let mut confirmation_link = reqwest::Url::parse(&raw_link).unwrap();
            // Let's make sure we don't call random APIs on the web
            assert_eq!(confirmation_link.host_str().unwrap(), "127.0.0.1");
            confirmation_link.set_port(Some(self.port)).unwrap();
            confirmation_link
        };

        let html = get_link(&body["HtmlBody"].as_str().unwrap());
        let plain_text = get_link(&body["TextBody"].as_str().unwrap());

        ConfirmationLinks { html, plain_text }
    }

    pub async fn post_newsletters(&self, body: serde_json::Value) -> reqwest::Response {
        let (username, password) = self.test_user().await;

        reqwest::Client::new()
            .post(&format!("{}/newsletters", &self.address))
            // Random credentials!
            // `reqwest` does all the encoding/formatting heavy-lifting for us.
            .basic_auth(username, Some(password))
            .json(&body)
            .send()
            .await
            .expect("Failed to execute request")
    }

    pub async fn test_user(&self) -> (String, String) {
        let row = sqlx::query!("SELECT username, password_hash FROM users LIMIT 1",)
            .fetch_one(&self.db_pool)
            .await
            .expect("Failed to create test users.");

        (row.username, row.password_hash)
    }

    pub async fn post_login<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
            .post(&format!("{}/login", &self.address))
            // This `reqwest` method makes sure that the body is URL-encoded and the `Content-Type`
            // header is set accordingly.
            .form(body)
            .send()
            .await
            .expect("Failed to execute request.")
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

/// We are running tests, so it is not worth it to propagate errors: if we fail to perform the required
/// setup we can just panic and crash all the things.
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

    let test_app = TestApp {
        address,
        db_pool: startup::get_connection_pool(&configuration.database),
        email_server,
        port,
        test_user: TestUser::generate(),
    };

    test_app.test_user.store(&test_app.db_pool).await;
    test_app
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

pub(crate) struct TestUser {
    pub(crate) user_id: Uuid,
    pub(crate) username: String,
    pub(crate) password: String,
}

impl TestUser {
    pub fn generate() -> Self {
        Self {
            user_id: Uuid::new_v4(),
            username: Uuid::new_v4().to_string(),
            password: Uuid::new_v4().to_string(),
        }
    }

    async fn store(&self, pool: &PgPool) {
        let salt = SaltString::generate(&mut rand::thread_rng());
        // We don't care about the exact Argon2 parameters here given that it's for testing purposes!
        let password_hash = Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            Params::new(15000, 2, 1, None).unwrap(),
        )
        .hash_password(self.password.as_bytes(), &salt)
        .unwrap()
        .to_string();

        sqlx::query!(
            "INSERT INTO users (user_id, username, password_hash)\
            VALUES ($1, $2, $3)",
            self.user_id,
            self.username,
            password_hash,
        )
        .execute(pool)
        .await
        .expect("Failed to store test user.");
    }
}
