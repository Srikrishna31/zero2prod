use crate::session_state::TypedSession;
use crate::utils::{e500, see_other};
use actix_web::http::header::ContentType;
use actix_web::{web, HttpResponse};
use tera::{Context, Tera};

pub async fn change_password_form(
    templates: web::Data<&Tera>,
    session: TypedSession,
) -> Result<HttpResponse, actix_web::Error> {
    if session.get_user_id().map_err(e500)?.is_none() {
        return Ok(see_other("/login"));
    }

    let context = Context::new();
    Ok(HttpResponse::Ok().content_type(ContentType::html()).body(
        templates
            .render("change_password_form.html", &context)
            .unwrap(),
    ))
}
