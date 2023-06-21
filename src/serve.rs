use std::{collections::HashSet, net::SocketAddr, sync::Arc};

use axum::{response::Redirect, routing::get, Json, Server};
use error_stack::{IntoReport, Report, Result, ResultExt};
use ory_hydra_client::models::{AcceptOAuth2ConsentRequest, AcceptOAuth2ConsentRequestSession};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tower_http::trace::TraceLayer;
use url::Url;

use crate::{
    cache::{SchemaCache, SchemaId},
    schema::Scope,
};

type SharedState = Arc<State>;

#[derive(Debug)]
struct State {
    kratos: ory_kratos_client::apis::configuration::Configuration,
    hydra: ory_hydra_client::apis::configuration::Configuration,

    cache: SchemaCache,
}

#[derive(Debug, Copy, Clone, Error)]
pub(crate) enum Error {
    #[error("API error to Hydra")]
    Hydra,
    #[error("API error to Kratos")]
    Kratos,
    #[error("request does not contain subject")]
    SubjectMissing,
    #[error("unable to fetch schema from Kratos")]
    IdentitySchema,
}

async fn handle_consent(state: &State, challenge: &str) -> Result<Redirect, Error> {
    let request =
        ory_hydra_client::apis::o_auth2_api::get_o_auth2_consent_request(&state.hydra, challenge)
            .await
            .into_report()
            .change_context(Error::Hydra)?;

    tracing::debug!(?request, "fetched consent request from hydra");

    // fetch all info from kratos
    let subject = request
        .subject
        .ok_or_else(|| Report::new(Error::SubjectMissing))?;

    let identity =
        ory_kratos_client::apis::identity_api::get_identity(&state.kratos, &subject, None)
            .await
            .into_report()
            .change_context(Error::Kratos)?;

    tracing::debug!(?identity, "fetched identity from kratos");

    let schema = state
        .cache
        .fetch(&state.kratos, &SchemaId::new(identity.schema_id))
        .await
        .change_context(Error::IdentitySchema)?;

    let scopes: HashSet<_> = request
        .requested_scope
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(Scope::new)
        .collect();

    let session = identity
        .traits
        .map(|traits| schema.resolve(&traits, &scopes));

    let (id_token, access_token) = if let Some(session) = session {
        (Some(session.id_token), Some(session.access_token))
    } else {
        (None, None)
    };

    tracing::debug!(?id_token, ?access_token, "resolved session");

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ConsentQuery {
    consent_challenge: String,
}

async fn consent(
    axum::extract::State(state): axum::extract::State<SharedState>,
    query: axum::extract::Query<ConsentQuery>,
) -> core::result::Result<Redirect, Json<Report<Error>>> {
    handle_consent(&state, &query.consent_challenge)
        .await
        .map_err(Json)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LogoutQuery {
    logout_challenge: String,
}

async fn logout(
    axum::extract::State(state): axum::extract::State<SharedState>,
    query: axum::extract::Query<LogoutQuery>,
) -> core::result::Result<Redirect, Json<Report<Error>>> {
    // for now, we just accept the logout request, in the future we might want to also enable asking
    // the user
    let request = ory_hydra_client::apis::o_auth2_api::get_o_auth2_logout_request(
        &state.hydra,
        &query.logout_challenge,
    )
    .await
    .into_report()
    .change_context(Error::Hydra)
    .map_err(Json)?;

    // TODO: unsure if sid or subject
    if let Some(sid) = request.sid {
        ory_kratos_client::apis::identity_api::delete_identity_sessions(&state.kratos, &sid)
            .await
            .into_report()
            .change_context(Error::Kratos)
            .map_err(Json)?;
    };

    let response = ory_hydra_client::apis::o_auth2_api::accept_o_auth2_logout_request(
        &state.hydra,
        &query.logout_challenge,
    )
    .await
    .into_report()
    .change_context(Error::Hydra)
    .map_err(Json)?;

    Ok(Redirect::to(&response.redirect_to))
}

#[derive(Debug)]
pub(crate) struct Config {
    pub(crate) kratos_url: Url,

    pub(crate) hydra_url: Url,

    pub(crate) direct_mapping: bool,
    pub(crate) keyword: String,
}

fn setup(config: Config) -> State {
    let kratos = ory_kratos_client::apis::configuration::Configuration {
        base_path: config.kratos_url.as_str().trim_end_matches('/').to_owned(),
        ..Default::default()
    };

    let hydra = ory_hydra_client::apis::configuration::Configuration {
        base_path: config.hydra_url.as_str().trim_end_matches('/').to_owned(),
        ..Default::default()
    };

    let cache = SchemaCache::new(config.keyword, config.direct_mapping);

    State {
        kratos,
        hydra,
        cache,
    }
}

pub(crate) async fn run(address: SocketAddr, config: Config) -> Result<(), Error> {
    let state = setup(config);
    let state = Arc::new(state);

    let router = axum::Router::new()
        .route("/consent", get(consent))
        .route("/logout", get(logout))
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    Server::bind(&address)
        .serve(router.into_make_service())
        .await
        .expect("should run forever-ish");

    Ok(())
}
