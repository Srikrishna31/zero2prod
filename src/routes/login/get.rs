use crate::routes::LoginError;
use actix_web::http::header::ContentType;
use actix_web::{web, HttpResponse};
use anyhow::Context as anyhow_ctx;
use tera::{Context, Tera};

#[derive(serde::Deserialize)]
pub struct QueryParams {
    error: Option<String>,
}

pub async fn login_form(
    query: web::Query<QueryParams>,
    templates: web::Data<&Tera>,
) -> Result<HttpResponse, LoginError> {
    let error_html = match query.0.error {
        None => "".into(),
        Some(error_message) => format!("<p><i>{error_message}</i><p>"),
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
