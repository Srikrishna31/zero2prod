use crate::authentication::UserId;
use crate::session_state::TypedSession;
use crate::utils::see_other;
use actix_web::{web, HttpResponse};
use actix_web_flash_messages::FlashMessage;

/// Invalid user validation is done by the middleware.
pub async fn log_out(
    userid: web::ReqData<UserId>,
    session: TypedSession,
) -> Result<HttpResponse, actix_web::Error> {
    let _user_id = userid.into_inner();
    session.log_out();
    FlashMessage::info("You have successfully logged out.").send();
    Ok(see_other("/login"))
}
