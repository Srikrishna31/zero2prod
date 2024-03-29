pub mod authentication;
pub mod configuration;
pub mod domain;
pub mod email_client;
mod idempotency;
pub mod issue_delivery_worker;
pub mod routes;
pub mod session_state;
pub mod startup;
pub mod telemetry;
mod utils;

extern crate tera;
