use tokio::net::TcpListener;
use zero2prod::run;
use actix_web::dev::Server;

#[tokio::main]
async fn main()  {
    let listener = TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind random port");

    let port = listener.local_addr().unwrap().port();

    println!("Running the server on: {port}");
    // Bubble up the io::Error if we failed to bind the address
    // Otherwise call .await on our Server
    run(listener).await?

}
