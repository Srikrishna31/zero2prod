use crate::{email_client::EmailClient, routes};
use actix_web::{dev::Server, web, App, HttpServer};
use sqlx::PgPool;
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

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
pub fn run(
    listener: TcpListener,
    db_pool: PgPool,
    email_client: EmailClient,
) -> Result<Server, std::io::Error> {
    // Wrap the connection in a smart pointer
    let db_pool = web::Data::new(db_pool);
    let email_client = web::Data::new(email_client);
    let server = HttpServer::new(move || {
        App::new()
            // Middlewares are added using the `wrap` method on `App`
            // Instead of `Logger::default`
            .wrap(TracingLogger::default())
            .route("/health_check", web::get().to(routes::health_check))
            .route("/subscriptions", web::post().to(routes::subscribe))
            // Register the connection as part of the application state
            .app_data(db_pool.clone())
            .app_data(email_client.clone())
    })
    .listen(listener)?
    .run();

    Ok(server)
}
