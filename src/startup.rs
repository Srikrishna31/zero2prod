use crate::authentication::reject_anonymous_users;
use crate::configuration::{DatabaseSettings, Settings};
use crate::{email_client::EmailClient, routes};
use actix_session::{storage::RedisSessionStore, SessionMiddleware};
use actix_web::{cookie::Key, dev::Server, web, web::Data, App, HttpServer};
use actix_web_flash_messages::{storage::CookieMessageStore, FlashMessagesFramework};
use actix_web_lab::middleware::from_fn;
use once_cell::sync::Lazy;
use secrecy::{ExposeSecret, Secret};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::net::TcpListener;
use tera::Tera;
use tracing_actix_web::TracingLogger;

pub fn get_connection_pool(configuration: &DatabaseSettings) -> PgPool {
    PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect_lazy_with(configuration.with_db())
}

pub struct Application {
    port: u16,
    server: Server,
}

#[derive(Clone)]
pub struct HmacSecret(pub Secret<String>);

/// We need to define a wrapper type in order to retrieve the URL in the `subscribe` handler.
/// Retrieval from the context, in actix-web, is type-based: using a raw `String` would expose us to
/// conflicts.
#[derive(Debug)]
pub struct ApplicationBaseUrl(pub String);

impl Application {
    pub async fn build(configuration: Settings) -> Result<Self, anyhow::Error> {
        let connection_pool = get_connection_pool(&configuration.database);

        let sender_email = configuration
            .email_client
            .sender()
            .expect("Invalid sender email address");

        let timeout = configuration.email_client.timeout();
        let email_client = EmailClient::new(
            &configuration.email_client.base_url,
            sender_email,
            configuration.email_client.authorization_token.clone(),
            timeout,
        )
        .expect("Unable to build email client");

        let address = format!(
            "{}:{}",
            configuration.application.host, configuration.application.port
        );

        let listener = TcpListener::bind(&address)?;
        //Retrieve the port assigned to us by the OS
        let port = listener.local_addr().unwrap().port();
        let server = run(
            listener,
            connection_pool,
            email_client,
            configuration.application.base_url,
            HmacSecret(configuration.application.hmac_secret),
            configuration.redis_uri,
        )
        .await?;

        // We "save" the bound port in one of `Application`'s fields.
        Ok(Self { port, server })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// A more expressive name that makes it clear that this function only returns when the application
    /// is stopped.
    pub async fn run_until_stopped(self) -> Result<(), std::io::Error> {
        self.server.await
    }
}

/// # Observability
///
/// The only thing we can rely on to understand and debug an unknown unknown is **telemetry data**:
/// information about our running applications that is collected automatically and can be later
/// inspected to answer questions about the state of the system at a certain point in time. The goal
/// is to have an **observable application**.
///
/// Observability is about being able to ask arbitrary questions about your environment without - and
/// this is the key part - having to know ahead of time what you wanted to ask.
///
/// "arbitrary" is a strong word - as all absolute statements, it might require an unreasonable
/// investment of both time and money if we are to interpret it literally.
///
/// In practice we will also happily settle for an application that is *sufficiently* observable to
/// enable us to deliver the level of service we promised to our users.
///
/// In a nutshell, to build an observable system we need:
/// * to instrument our application to collect high-quality telemetry data;
/// * access to tools and systems to efficiently slice, dice and manipulate the data to find answers
/// to our questions.
///
/// # Logging
/// Logs are the most common type of telemetry data. The go-to crate for logging in Rust is `log`.
/// `log` provides five macros: `trace`, `debug`, `info`, `warn` and `error`.
/// They all do the same thing - emit a log record - but each of them uses a different **log level**,
/// as the naming implies.
///
/// *trace* is the lowest level: trace-level logs are often extremely verbose and have a low signal-to
/// -noise ratio. We then have, in increasing order of severity, *debug*, *info*, *warn* and *error*.
/// Error-level logs are used to report serious failures that might have user impact(e.g. we failed
/// to handle an incoming request or a query to the database timed out).
///
/// Choosing what information should be logged about the execution of a particular function is often
/// a *local* decision: it is enough to look at the function to decide what deserves to be captured
/// in a log record. This enables libraries to be instrumented effectively, extending the reach of our
/// telemetry outside the boundaries of the code we have written first-hand.
async fn run(
    listener: TcpListener,
    db_pool: PgPool,
    email_client: EmailClient,
    base_url: String,
    hmac_secret: HmacSecret,
    redis_uri: Secret<String>,
) -> Result<Server, anyhow::Error> {
    // Wrap the connection in a smart pointer
    let db_pool = web::Data::new(db_pool);
    let email_client = web::Data::new(email_client);
    let base_url = Data::new(ApplicationBaseUrl(base_url));
    let templates = Data::new(Lazy::force(&TEMPLATES));
    let message_store =
        CookieMessageStore::builder(Key::from(hmac_secret.0.expose_secret().as_bytes())).build();
    let message_framework = FlashMessagesFramework::builder(message_store).build();
    let secret_key = Key::from(hmac_secret.0.expose_secret().as_bytes());
    let redis_store = RedisSessionStore::new(redis_uri.expose_secret()).await?;

    let server = HttpServer::new(move || {
        App::new()
            // Middlewares are added using the `wrap` method on `App`
            .wrap(message_framework.clone())
            // Instead of `Logger::default`
            .wrap(TracingLogger::default())
            .wrap(SessionMiddleware::new(
                redis_store.clone(),
                secret_key.clone(),
            ))
            .route("/", web::get().to(routes::home))
            .route("/login", web::get().to(routes::login_form))
            .route("/login", web::post().to(routes::login))
            .route("/health_check", web::get().to(routes::health_check))
            .route("/newsletters", web::post().to(routes::publish_newsletter))
            .route("/subscriptions", web::post().to(routes::subscribe))
            .route("/subscriptions/confirm", web::get().to(routes::confirm))
            .service(
                web::scope("/admin")
                    .wrap(from_fn(reject_anonymous_users))
                    .route("/dashboard", web::get().to(routes::admin_dashboard))
                    .route(
                        "/newsletters",
                        web::get().to(routes::publish_newsletter_form),
                    )
                    .route("/newsletters", web::post().to(routes::publish_newsletter))
                    .route("/password", web::get().to(routes::change_password_form))
                    .route("/password", web::post().to(routes::change_password))
                    .route("/logout", web::post().to(routes::log_out)),
            )
            // Register the connection as part of the application state
            .app_data(db_pool.clone())
            .app_data(email_client.clone())
            .app_data(base_url.clone())
            .app_data(templates.clone())
            .app_data(Data::new(hmac_secret.clone()))
    })
    .listen(listener)?
    .run();

    Ok(server)
}

static TEMPLATES: Lazy<Tera> = Lazy::new(|| {
    let mut tera = match Tera::new("templates/**/*") {
        Ok(t) => t,
        Err(e) => {
            println!("Parsing error(s): {e}");
            ::std::process::exit(1); //should we exit the process?
        }
    };
    //Disable auto-escaping for now.
    tera.autoescape_on(vec![]);
    let template_names: Vec<&str> = tera.get_template_names().collect();
    println!("Registered templates: {:?}", template_names);
    tera
});
