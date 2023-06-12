use axum::response::Redirect;
use error_stack::{IntoReport, Report, Result, ResultExt};
use ory_hydra_client::models::{
    AcceptOAuth2ConsentRequest, AcceptOAuth2ConsentRequestSession, OAuth2ConsentSession,
};
use thiserror::Error;

use crate::schema::{Cache, Configuration};

struct State {
    kratos: ory_kratos_client::apis::configuration::Configuration,
    hydra: ory_hydra_client::apis::configuration::Configuration,
    cache: Cache,
    config: Configuration,
}

#[derive(Debug, Copy, Clone, Error)]
enum Error {
    #[error("API error to Hydra")]
    Hydra,
    #[error("API error to Kratos")]
    Kratos,
    #[error("request does not contain subject")]
    SubjectMissing,
}

async fn handle_consent(state: &State, challenge: &str) -> Result<Redirect, Error> {
    let request =
        ory_hydra_client::apis::o_auth2_api::get_o_auth2_consent_request(&state.hydra, challenge)
            .await
            .into_report()
            .change_context(Error::Hydra)?;

    // fetch all info from kratos
    let subject = request
        .subject
        .ok_or_else(|| Report::new(Error::SubjectMissing))?;

    let identity =
        ory_kratos_client::apis::identity_api::get_identity(&state.kratos, &subject, None)
            .await
            .into_report()
            .change_context(Error::Kratos)?;

    let session = identity
        .traits
        .map(|traits| state.config.resolve_all(&traits, &state.cache));

    let (id_token, access_token) = if let Some(session) = session {
        (Some(session.id_token), Some(session.access_token))
    } else {
        (None, None)
    };

    // we automatically skip consent, always
    let response = ory_hydra_client::apis::o_auth2_api::accept_o_auth2_consent_request(
        &state.hydra,
        challenge,
        Some(&AcceptOAuth2ConsentRequest {
            grant_access_token_audience: request.requested_access_token_audience,
            grant_scope: request.requested_scope,
            handled_at: None,
            remember: None,
            remember_for: None,
            session: Some(Box::new(AcceptOAuth2ConsentRequestSession {
                access_token,
                id_token,
            })),
        }),
    )
    .await
    .into_report()
    .change_context(Error::Hydra)?;

    Ok(Redirect::to(&response.redirect_to))
}
