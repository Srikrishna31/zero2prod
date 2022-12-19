use crate::authentication::UserId;
use crate::utils::e500;
use actix_web::http::header::{ContentType, LOCATION};
use actix_web::{web, HttpResponse};
use anyhow::Context;
use sqlx::PgPool;
use tera::{Context as tcontext, Tera};
use uuid::Uuid;

pub async fn admin_dashboard(
    user_id: web::ReqData<UserId>,
    pool: web::Data<PgPool>,
    templates: web::Data<&Tera>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = user_id.into_inner();
    let username = match get_username(*user_id, &pool).await.map_err(e500) {
        Ok(user) => user,
        Err(_) => {
            return Ok(HttpResponse::SeeOther()
                .insert_header((LOCATION, "/login"))
                .finish())
        }
    };

    let mut template_context = tcontext::new();
    template_context.insert("username", &username);
    let html_body = templates
        .render("admin_dashboard.html", &template_context)
        .context("Error rendering admin_dashboard html")
        .map_err(e500)?;

    Ok(HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(html_body))
}

#[tracing::instrument(name = "Get username", skip(pool))]
pub(in crate::routes) async fn get_username(
    user_id: Uuid,
    pool: &PgPool,
) -> Result<String, anyhow::Error> {
    let row = sqlx::query!(
        r#"
        SELECT username FROM users WHERE user_id = $1
        "#,
        user_id,
    )
    .fetch_one(pool)
    .await
    .context("Failed to perform a query to retrieve a username.")?;

    Ok(row.username)
}
