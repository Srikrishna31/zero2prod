use std::fmt::{Debug, Display};
use tokio::task::JoinError;
use zero2prod::issue_delivery_worker::run_worker_until_stopped;
use zero2prod::{configuration, startup::Application, telemetry};

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
async fn main() -> anyhow::Result<()> {
    //Panic if we can't read configuration
    let configuration = configuration::get_configuration().expect("Failed to read configuration");
    // We have removed the hard-coded `8000` - it's now coming from our settings!
    let address = format!(
        "{}:{}",
        configuration.application.host, configuration.application.port
    );

    let subscriber = telemetry::get_subscriber("zero2prod".into(), "info".into(), std::io::stdout);
    telemetry::init_subscriber(subscriber);

    let application = Application::build(configuration.clone()).await?;
    let port = application.port();
    let application_task = tokio::spawn(application.run_until_stopped());
    let worker_task = tokio::spawn(run_worker_until_stopped(configuration));

    tokio::select! {
        o = application_task => report_exit("API", o),
        o = worker_task => report_exit("Background worker", o),
    };

    println!("Running the server on: {address}:{port}");

    Ok(())
}

fn report_exit(task_name: &str, outcome: Result<Result<(), impl Debug + Display>, JoinError>) {
    match outcome {
        Ok(Ok(())) => {
            tracing::info!("{task_name} has exited")
        }
        Ok(Err(e)) => {
            tracing::error!(error.cause_chain = ?e, error.message = %e, "{task_name} failed")
        }
        Err(e) => {
            tracing::error!(error.cause_chain = ?e, error.message = %e, "{task_name} failed to complete")
        }
    }
}
