use crate::authentication;
use crate::authentication::{AuthError, Credentials};
use crate::routes::error_chain_fmt;
use actix_web::http::header::LOCATION;
use actix_web::http::StatusCode;
use actix_web::{error::InternalError, web, HttpResponse, ResponseError};
use secrecy::Secret;
use sqlx::PgPool;
use std::fmt::Formatter;

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
) -> Result<HttpResponse, InternalError<LoginError>> {
    let credentials = Credentials {
        username: form.0.username,
        password: form.0.password,
    };

    tracing::Span::current().record("username", &tracing::field::display(&credentials.username));

    match authentication::validate_credentials(credentials, &pool).await {
        Ok(user_id) => {
            tracing::Span::current().record("user_id", &tracing::field::display(&user_id));
            Ok(HttpResponse::SeeOther()
                .insert_header((LOCATION, "/"))
                .finish())
        }
        Err(e) => {
            let e = match e {
                AuthError::InvalidCredentials(_) => LoginError::AuthError(e.into()),
                AuthError::UnexpectedError(_) => LoginError::UnexpectedError(e.into()),
            };
            let response = HttpResponse::SeeOther()
                .insert_header((LOCATION, "/login"))
                .insert_header(("Set-Cookie", format!("_flash={e}")))
                .finish();
            //Save the error reporting in the logs for debugging purposes.
            Err(InternalError::from_response(e, response))
        }
    }
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
        StatusCode::INTERNAL_SERVER_ERROR //Maintained for login_form function template rendering failure.
    }
}
