use crate::authentication;
use crate::authentication::{AuthError, Credentials};
use crate::routes::error_chain_fmt;
use actix_web::body::BoxBody;
use actix_web::http::header::LOCATION;
use actix_web::{web, HttpResponse, ResponseError};
use hmac::{Hmac, Mac};
use secrecy::Secret;
use sqlx::PgPool;
use std::fmt::Formatter;
use actix_web::http::StatusCode;

#[allow(dead_code)]
#[derive(serde::Deserialize)]
pub struct FormData {
    username: String,
    password: Secret<String>,
}

/// # Redirect on Success
/// A redirect response requires two elements:
/// * a redirect status code;
/// * a `Location` header, set to the URL we want to redirect to.
///
/// All redirect status codes are in the 3xx range -  we need to choose the most appropriate one
/// depending on the HTTP verb and the semantic meaning we want to communicate(e.g. temporary vs
/// permanent redirection).
#[tracing::instrument(
    skip(form, pool),
    fields(username=tracing::field::Empty, user_id=tracing::field::Empty)
)]
pub async fn login(
    form: web::Form<FormData>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, LoginError> {
    let credentials = Credentials {
        username: form.0.username,
        password: form.0.password,
    };

    tracing::Span::current().record("username", &tracing::field::display(&credentials.username));

    let user_id = authentication::validate_credentials(credentials, &pool)
        .await
        .map_err(|e| match e {
            AuthError::InvalidCredentials(_) => LoginError::AuthError(e.into()),
            AuthError::UnexpectedError(_) => LoginError::AuthError(e.into()),
        })?;

    tracing::Span::current().record("user_id", &tracing::field::display(&user_id));

    Ok(HttpResponse::SeeOther()
        .insert_header((LOCATION, "/"))
        .finish())
}

#[derive(thiserror::Error)]
pub enum LoginError {
    #[error("Authentication failed")]
    AuthError(#[source] anyhow::Error),
    #[error("Something went wrong")]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for LoginError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for LoginError {
    fn status_code(&self) -> StatusCode {
        StatusCode::SEE_OTHER
    }

    fn error_response(&self) -> HttpResponse<BoxBody> {
        let encoded_error = urlencoding::Encoded::new(self.to_string());
        HttpResponse::build(self.status_code())
            .insert_header((LOCATION, format!("/login?error={encoded_error}")))
            .finish()
    }
}
