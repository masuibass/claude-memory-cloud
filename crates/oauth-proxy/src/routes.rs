use axum::{Router, routing::{get, post}};

use crate::handlers::{authorize, callback, register, token};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register::handle_register))
        .route("/authorize", get(authorize::handle_authorize))
        .route("/callback", get(callback::handle_callback))
        .route("/token", post(token::handle_token))
}
