use error_stack::{IntoReport, Result, ResultExt};
use ory_kratos_client::apis::configuration::Configuration;
use schemars::schema::SchemaObject;
use thiserror::Error;

use crate::{
    schema::{Cache, ImplicitScope},
    serve::Config,
};

#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error("error while fetching from Kratos")]
    Kratos,
    #[error("schema is malformed")]
    IdentitySchemaMalformed,
}

pub(crate) async fn fetch(
    config: &Configuration,
    id: &str,
    direct_mapping: bool,
) -> Result<(Cache, crate::schema::Configuration), Error> {
    // fetch the identity schema from kratos
    let identity_schema = ory_kratos_client::apis::identity_api::get_identity_schema(&config, &id)
        .await
        .into_report()
        .change_context(Error::Kratos)?;

    let schema: SchemaObject = serde_json::from_value(identity_schema)
        .into_report()
        .change_context(Error::IdentitySchemaMalformed)?;

    let cache = ImplicitScope::find(schema.clone(), vec![]);
    let cache = Cache::new(cache);

    let config = crate::schema::Configuration::from_root(schema, &cache, direct_mapping);

    Ok((cache, config))
}

pub(crate) async fn run(config: Config) -> Result<(), Error> {
    todo!()
}
