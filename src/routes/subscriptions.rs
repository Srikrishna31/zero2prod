use actix_web::{web, HttpResponse};
use chrono;
use sqlx::PgPool;
use unicode_segmentation::UnicodeSegmentation;
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
    if !is_valid_name(&form.name) {
        return HttpResponse::BadRequest().finish();
    }

    match insert_subscriber(&pool, &form).await {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[tracing::instrument(
    name = "Saving new subscriber details in the database",
    skip(form, pool)
)]
async fn insert_subscriber(pool: &PgPool, form: &FormData) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at)
        VALUES ($1, $2, $3, $4)
        "#,
        Uuid::new_v4(),
        form.email,
        form.name,
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

/// Returns `true` if the input satisfies all our validation constraints on subscriber names, `false`
/// otherwise
fn is_valid_name(s: &str) -> bool {
    // `.trim()` returns a view over the input `s` without trailing whitespace-like characters.
    // `.is_empty` checks if the view contains any character.
    let is_empty_or_whitespace = s.trim().is_empty();

    // A grapheme is defined by the Unicode standard as a "user-perceived" character: `a°` is a single
    // grapheme, but it is composed of two characters (`a` and `°`).
    //
    // `graphemes` returns an iterator over the graphemes in the input `s`. `true` specifies that we
    // want to use the extended grapheme definition set, the recommended one.
    let is_too_long = s.graphemes(true).count() > 256;

    // Iterate over all characters in the input `s` to check if any of them matches one of the characters
    // in the forbidden array.
    let forbidden_characters = ['/', '(', ')', '"', '<', '>', '\\', '{', '}'];
    let contains_forbiden_characters = s.chars().any(|g| forbidden_characters.contains(&g));

    !(is_empty_or_whitespace || is_too_long || contains_forbiden_characters)
}
