use crate::authentication::UserId;
use crate::domain::SubscriberEmail;
use crate::email_client::EmailClient;
use crate::idempotency::IdempotencyKey;
use crate::utils::{e400, e500, see_other};
use actix_web::{web, web::ReqData, HttpResponse};
use actix_web_flash_messages::FlashMessage;
use anyhow::Context;
use sqlx::PgPool;

#[derive(serde::Deserialize)]
pub struct FormData {
    title: String,
    text_content: String,
    html_content: String,
    idempotency_key: String,
}

/// # Idempotency
/// An API endpoint is retry-safe(or **idempotent**) if the caller has no way to **observe** if a
/// request has been sent to the server once or multiple times.
#[tracing::instrument(
    name = "Publish a newsletter issue",
    skip(form, pool, email_client, user_id)
    fields(user_id=%*user_id)
)]
pub async fn publish_newsletter(
    form: web::Form<FormData>,
    user_id: ReqData<UserId>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
) -> Result<HttpResponse, actix_web::Error> {
    // We must destructure the form to avoid upsetting the borrow-checker
    let FormData {title, text_content, html_content, idempotency_key } = form.0;
    let idempotency_key: IdempotencyKey = idempotency_key.try_into().map_err(e400)?;
    let subscribers = get_confirmed_subscribers(&pool).await.map_err(e500)?;
    for subscriber in subscribers {
        match subscriber {
            Ok(subscriber) => {
                email_client
                    .send_email(
                        &subscriber.email,
                        &title,
                        &html_content,
                        &text_content,
                    )
                    .await
                    .with_context(|| {
                        format!("Failed to send newsletter issue to {}", subscriber.email)
                    })
                    .map_err(e500)?;
            }
            Err(error) => {
                tracing::warn!(error.cause_chain = ?error, error.message=%error,
                "skipping a confirmed subscriber. Their stored contact details are invalid");
            }
        }
    }
    FlashMessage::info("The newsletter has been published!").send();
    Ok(see_other("/admin/newsletters"))
}

struct ConfirmedSubscriber {
    email: SubscriberEmail,
}

#[tracing::instrument(name = "Get confirmed subscriber", skip(pool))]
async fn get_confirmed_subscribers(
    pool: &PgPool, // We are returning a `Vec` of `Result`'s in the happy case. This allows the caller to bubble up
                   // errors due to network issues or other transient failures using the `?` operator, while the
                   // compiler forces them to handle the subtler mapping error.
                   // See https://sled.rs/errors.html for a deep-dive about this technique.
) -> Result<Vec<Result<ConfirmedSubscriber, anyhow::Error>>, anyhow::Error> {
    let confirmed_subscribers = sqlx::query!(
        r#"
        SELECT email FROM subscriptions WHERE status = 'confirmed'
        "#
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| match SubscriberEmail::parse(r.email) {
        Ok(email) => Ok(ConfirmedSubscriber { email }),
        Err(error) => Err(anyhow::anyhow!(error)),
    })
    .collect();

    Ok(confirmed_subscribers)
}
