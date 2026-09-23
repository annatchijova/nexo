//! Authentication: a bearer token resolved through a digest-backed credential
//! row belonging to an actor.
//!
//! This is a real, structured mechanism, not a hardcoded bypass — every
//! actor is a row with an ownership model already enforced by
//! `docs/APPLICATION_LAYER_CONTRACT.md`'s repository layer, so adding a
//! second actor later is "create another row," not a schema rewrite.
//! Every handler downstream only sees an already-authenticated `ActorRowId`,
//! never a raw credential or its digest. Credential rows can overlap during
//! rotation and be revoked independently; account-management endpoints remain
//! outside this slice.

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
        let token = bearer_token(parts)?;

        let actor = repository::find_actor_by_identity(&state.pool, &token)
            .await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "actor lookup failed"))?
            .ok_or((StatusCode::UNAUTHORIZED, "unknown token"))?;

        Ok(AuthenticatedActor(actor))
    }
}

/// The raw bearer token presented on this request, for the one handler
/// that needs it (revoking the credential currently in use). Every other
/// handler uses `AuthenticatedActor` alone and never sees the raw token —
/// this extractor does not perform the actor lookup itself, only repeats
/// the same header parsing, so adding it cannot widen what an ordinary
/// handler has access to.
#[derive(Clone, Debug)]
pub struct CurrentCredential(pub String);

impl FromRequestParts<AppState> for CurrentCredential {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        bearer_token(parts).map(CurrentCredential)
    }
}

fn bearer_token(parts: &Parts) -> Result<String, (StatusCode, &'static str)> {
    let header = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing Authorization header"))?;
    header
        .strip_prefix("Bearer ")
        .map(str::to_string)
        .ok_or((StatusCode::UNAUTHORIZED, "expected Bearer token"))
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
