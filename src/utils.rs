use actix_web::{HttpResponse, http::StatusCode};
use reqwest::header::LOCATION;

// Return an opaque 500 while preserving the error's root cause for logging.
pub(crate) fn e500<T>(e: T) -> actix_web::Error
where
    T: std::fmt::Debug + std::fmt::Display + 'static,
{
    actix_web::error::ErrorInternalServerError(e)
}

pub(crate) fn see_other(location: &str) -> HttpResponse {
    HttpResponse::SeeOther()
        .insert_header((LOCATION, location))
        .finish()
}

// Return a 400 with the user-representation of the validation error as body. The error root cause is
// preserved for logging purposes
pub fn e400<T: std::fmt::Debug + std::fmt::Display>(e: T) -> actix_web::Error
where T: std::fmt::Debug + std::fmt::Display + 'static
{
    actix_web::error::ErrorBadRequest(e)
}