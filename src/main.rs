use std::net::TcpListener;
use zero2prod::startup::run;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");

    let port = listener.local_addr().unwrap().port();

    println!("Running the server on: http://127.0.0.1:{port}");
    // Bubble up the io::Error if we failed to bind the address
    // Otherwise call .await on our Server

    run(listener)?.await?;

    Ok(())
}
