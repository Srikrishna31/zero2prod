use sqlx::PgPool;
use std::net::TcpListener;
use tracing::subscriber::set_global_default;
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_log::LogTracer;
use tracing_subscriber::{layer::SubscriberExt, EnvFilter, Registry};
use zero2prod::{configuration, startup};

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
    //Redirect all `log`'s events to our subscriber
    LogTracer::init().expect("Failed to set logger");

    // We are falling back to printing all spans at info-level or above if the RUST_LOG environment
    // variable has not been set.
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let formatting_layer = BunyanFormattingLayer::new(
        "zero2prod".into(),
        //Output the formatted spans to stdout.
        std::io::stdout,
    );

    // The `with` method is provided by `SubscriberExt`, an extension trait for `Subscriber` exposed
    // by `tracing_subscriber`
    let subscriber = Registry::default()
        .with(env_filter)
        .with(JsonStorageLayer)
        .with(formatting_layer);

    // `set_global_default` can be used by applications to specify what subscriber should be used to
    // process spans.
    set_global_default(subscriber).expect("Failed to set subscriber");

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
