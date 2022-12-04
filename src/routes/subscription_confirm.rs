use actix_web::{web, HttpResponse};

/// The `Parameters` struct defines all the query parameters that we *expect* to see in the incoming
/// request. It needs to implement `serde::Deserialize` to enable `actix-web` to build it from the
/// incoming request path. It is enough to add a function parameter of type `web::Query<Parameter>`
/// to confirm to instruct `actix-web` to only call the handler if the extraction was successful. If
/// the extraction failed, a `400 Bad Request` is automatically returned to the caller.
#[derive(serde::Deserialize)]
pub struct Parameters {
    subscription_token: String,
}

#[tracing::instrument(name = "Confirm a pending subscriber", skip(_parameters))]
pub async fn confirm(_parameters: web::Query<Parameters>) -> HttpResponse {
    HttpResponse::Ok().finish()
}
