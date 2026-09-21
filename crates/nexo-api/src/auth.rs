//! Authentication: a bearer token that *is* the actor's `external_identity`.
//!
//! This is a real, structured mechanism, not a hardcoded bypass — every
//! actor is a row with an ownership model already enforced by
//! `docs/APPLICATION_LAYER_CONTRACT.md`'s repository layer, so adding a
//! second actor later is "create another row," not a schema rewrite.
//! Swapping the token scheme itself (hashed API keys, OAuth, session
//! cookies) later only touches this file: every handler downstream only
//! ever sees an already-authenticated `ActorRowId`, never a raw credential.
//!
//! Known limitation, recorded rather than hidden: the token is compared to
//! the stored `external_identity` in plaintext (constant-time compare, but
//! not hashed at rest) — acceptable for the single-owner personal
//! deployment this round targets, not yet a credential system to expose to
//! untrusted multi-tenant traffic. See docs/API_CONTRACT.md.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;

use nexo_app::repository::{self, ActorRowId};

use crate::AppState;

#[derive(Clone, Copy, Debug)]
pub struct AuthenticatedActor(pub ActorRowId);

impl FromRequestParts<AppState> for AuthenticatedActor {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or((StatusCode::UNAUTHORIZED, "missing Authorization header"))?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or((StatusCode::UNAUTHORIZED, "expected Bearer token"))?;

        let actor = repository::find_actor_by_identity(&state.pool, token)
            .await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "actor lookup failed"))?
            .ok_or((StatusCode::UNAUTHORIZED, "unknown token"))?;

        Ok(AuthenticatedActor(actor))
    }
}

/// Loads the case and checks that `actor` owns it, in one place, so every
/// handler enforces ownership identically. Per
/// `docs/APPLICATION_LAYER_CONTRACT.md`'s actor/ownership model: no case is
/// readable or writable by anyone but its owner.
pub async fn authorize_case(
    pool: &repository::Pool,
    case: repository::CaseRowId,
    actor: ActorRowId,
) -> Result<(), (StatusCode, &'static str)> {
    let owner = repository::case_owner(pool, case)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "case lookup failed"))?
        .ok_or((StatusCode::NOT_FOUND, "case not found"))?;
    if owner != actor {
        // Same response as "not found" — do not reveal that a case with
        // this id exists but belongs to someone else.
        return Err((StatusCode::NOT_FOUND, "case not found"));
    }
    Ok(())
}
