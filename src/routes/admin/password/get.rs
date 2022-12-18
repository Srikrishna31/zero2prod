use crate::session_state::TypedSession;
use crate::utils::{e500, see_other};
use actix_web::http::header::ContentType;
use actix_web::{web, HttpResponse};
use actix_web_flash_messages::IncomingFlashMessages;
use std::fmt::Write;
use tera::{Context, Tera};

pub async fn change_password_form(
    templates: web::Data<&Tera>,
    session: TypedSession,
    flash_messages: IncomingFlashMessages,
) -> Result<HttpResponse, actix_web::Error> {
    if session.get_user_id().map_err(e500)?.is_none() {
        return Ok(see_other("/login"));
    }

    let mut error_message = String::new();
    for m in flash_messages.iter() {
        writeln!(error_message, "<p><i>{}</i></p>", m.content()).unwrap();
    }

    let mut context = Context::new();
    context.insert("error_message", &error_message);
    Ok(HttpResponse::Ok().content_type(ContentType::html()).body(
        templates
            .render("change_password_form.html", &context)
            .unwrap(),
    ))
}
