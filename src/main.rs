use std::net::TcpListener;
use zero2prod::configuration::get_configuration;
use zero2prod::startup::run;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    //Panic if we can't read configuration
    let configuration = get_configuration().expect("Failed to read configuration");

    // We have removed the hard-coded `8000` - it's now coming from our settings!
    let address = format!("127.0.0.1:{}", configuration.application_port);
    let listener = TcpListener::bind(address).expect("Failed to bind random port");

    let port = listener.local_addr().unwrap().port();

    println!("Running the server on: http://127.0.0.1:{port}");
    // Bubble up the io::Error if we failed to bind the address
    // Otherwise call .await on our Server

    run(listener)?.await?;

    Ok(())
}
