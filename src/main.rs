use sqlx::PgPool;
use std::net::TcpListener;
use zero2prod::{configuration, startup, telemetry};

/// # tracing-subscriber
/// `tracing-subscriber` does much more than providing us with a few handy subscribers. It introduces
/// another key trait into the picture, `Layer`
///
/// `Layer` makes it possible to build a *processing pipeline* for spans data: we are not forced to
/// provide an all encompassing subscriber that does everything we want; we can instead combine multiple
/// smaller layers to obtain the processing pipeline we need.
///
/// This substantially reduces duplication across in tracing ecosystem: people are focused on adding
/// new capabilities by churning out new layers rather than trying to build the best-possible-batteries
/// -included subscriber.
///
/// The cornerstone of the layering approach is `Registry`. `Registry` does not actually record traces
/// itself: instead, it collects and stores span data that is exposed to any layer wrapping it. The
/// `Registry` is responsible for storing span metadata, recording relationships between spans, and
/// tracking which spans are active and which are closed.
///
/// Downstream layers can piggyback on `Registry`'s functionality and focus on their purpose: filtering
/// what spans should be processed, formatting span data, shipping span data to remote systems, etc.
#[tokio::main]
async fn main() -> std::io::Result<()> {
    let subscriber = telemetry::get_subscriber("zero2prod".into(), "info".into());
    telemetry::init_subscriber(subscriber);
    //Panic if we can't read configuration
    let configuration = configuration::get_configuration().expect("Failed to read configuration");
    let connection_pool = PgPool::connect(&configuration.database.connection_string())
        .await
        .expect("Failed to connect to Postgres");
    // We have removed the hard-coded `8000` - it's now coming from our settings!
    let address = format!("127.0.0.1:{}", configuration.application_port);
    let listener = TcpListener::bind(address).expect("Failed to bind random port");

    let port = listener.local_addr().unwrap().port();

    println!("Running the server on: http://127.0.0.1:{port}");
    // Bubble up the io::Error if we failed to bind the address
    // Otherwise call .await on our Server

    startup::run(listener, connection_pool)?.await?;

    Ok(())
}
