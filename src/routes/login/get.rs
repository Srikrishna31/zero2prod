use crate::routes::LoginError;
use actix_web::http::header::ContentType;
use actix_web::{web, HttpResponse};
use anyhow::Context as anyhow_ctx;
use tera::{Context, Tera};

#[derive(serde::Deserialize)]
pub struct QueryParams {
    error: Option<String>,
}

/// # Cross-Site-Scripting(XSS)
/// Query parameters are not private - our backend server cannot prevent users from tweaking the URL.
/// When the attacker injects HTML fragments or JavaScript snippets into a trusted website by
/// exploiting dynamic content built from untrusted sources - e.g. user inputs, query parameters etc.
/// HTML entity encoding prevents the insertion of further HTML elements by escaping the characters
/// required to define them.
///
/// # Message Authentication Codes
/// We need a mechanism to verify that the query parameters have been set by our API and that they
/// have not been altered by a third party.
///
/// This is known as **message authentication** - it guarantees that the message has not been modified
/// in transit (**integrity**) and it allows you to verify the identity of the sender (**data origin
/// authentication**).
///
/// Message authentication codes (MACs) are a common technique to provide message authentication - a
/// *tag* is added to the message allowing verifiers to check its integrity and origin.
pub async fn login_form(
    query: web::Query<QueryParams>,
    templates: web::Data<&Tera>,
) -> Result<HttpResponse, LoginError> {
    let error_html = match query.0.error {
        None => "".into(),
        Some(error_message) => format!(
            "<p><i>{}</i><p>",
            htmlescape::encode_minimal(&error_message)
        ),
    };

    let mut template_context = Context::new();
    template_context.insert("error_html", &error_html);
    let html_body = templates
        .render("login.html", &template_context)
        .context("Error rendering login html")
        .map_err(|e| LoginError::UnexpectedError(e.into()))?;

    Ok(HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(html_body))
}
