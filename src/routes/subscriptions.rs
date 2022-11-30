use crate::domain::{NewSubscriber, SubscriberName};
use actix_web::{web, HttpResponse};
use chrono;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(serde::Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

/// actix-web uses a *type-map* to represent its application state: a `HashMap` that stores arbitrary
/// data (using the `Any` type) against their unique type identifier(obtained via `TypeId::of`).
/// `web::Data`, when a new request comes in, computes the `TypeId` of the type you specified in the
/// signature (in this case `PgConnection`) and checks if there is a record corresponding to it in the
/// type-map. If there is one, it casts the retrieved `Any` value to the type you specified(`TypeId`
/// is unique, nothing to worry about) and passes it to your handler.
///
/// It is an interesting technique to perform what in other language ecosystems might be referred to
/// as *dependency injection*.
///
/// # Cleaning up Instrumentation Code - tracing::Instrument
///
/// We'd like all operations within subscribe to happen within the context of request_span. In other
/// words, we'd like to *wrap* the `subscribe` function in a span. `#[tracing::instrument]` creates
/// a span at the beginning of the function invocation and automatically attaches all arguments passed
/// to the function to the context of the span - in our case, `form` and `pool`. Often function arguments
/// won't be displayable on log records (e.g. `pool`) or we'd like to specify more explicitly what
/// should/how they should be captured (e.g. naming each field of `form`) - we can explicitly tell
/// `tracing` to ignore them using the skip directive.
///
/// `name` can be used to specify the message associated to the function span - if omitted, it defaults
/// to the function name.
///
/// We can also enrich the span's context using the `fields` directive. It leverages the same syntax
/// we have already for the info_span! macro.
///
/// The result is quite nice: all instrumentation concerns are visually separated by execution
/// concerns - the first are dealt with in a procedural macro that "decorates" the function declaration,
/// while the function body focuses on the actual business logic.
#[tracing::instrument(
    name = "Adding a new subscriber",
    skip(form, pool),
    fields(
        subscriber_email = %form.email,
        subscriber_name = %form.name
    )
)]
pub async fn subscribe(
    form: web::Form<FormData>,
    // Retrieving a connection from the application state!
    pool: web::Data<PgPool>,
) -> HttpResponse {
    let name = match SubscriberName::parse(form.0.name) {
        Ok(name) => name,
        //Return early if the name is invalid, with a 400.
        Err(_) => return HttpResponse::BadRequest().finish(),
    };
    let new_subscriber = NewSubscriber {
        email: form.0.email,
        name,
    };

    match insert_subscriber(&pool, &new_subscriber).await {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[tracing::instrument(
    name = "Saving new subscriber details in the database",
    skip(new_subscriber, pool)
)]
async fn insert_subscriber(
    pool: &PgPool,
    new_subscriber: &NewSubscriber,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at)
        VALUES ($1, $2, $3, $4)
        "#,
        Uuid::new_v4(),
        new_subscriber.email,
        new_subscriber.name.as_ref(),
        chrono::Utc::now()
    )
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
        // Using the `?` operator to return early if the function failed, returning a sqlx::Error
    })?;

    Ok(())
}
