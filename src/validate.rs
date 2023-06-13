use error_stack::{IntoReport, Result, ResultExt};
use ory_kratos_client::apis::configuration::Configuration;
use schemars::schema::SchemaObject;
use thiserror::Error;

use crate::{cache::ScopeCache, schema::ImplicitScope, serve::Config};

#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error("error while fetching from Kratos")]
    Kratos,
    #[error("schema is malformed")]
    IdentitySchemaMalformed,
}

pub(crate) async fn fetch(
    config: &Configuration,
    keyword: &str,
    id: &str,
    direct_mapping: bool,
) -> Result<(ScopeCache, crate::schema::ScopeConfig), Error> {
    // fetch the identity schema from kratos
    let identity_schema = ory_kratos_client::apis::identity_api::get_identity_schema(&config, &id)
        .await
        .into_report()
        .change_context(Error::Kratos)?;

    let schema: SchemaObject = serde_json::from_value(identity_schema)
        .into_report()
        .change_context(Error::IdentitySchemaMalformed)?;

    let cache = ImplicitScope::find(keyword, schema.clone(), vec![]);
    let cache = ScopeCache::new(cache);

    let config = crate::schema::ScopeConfig::from_root(keyword, schema, &cache, direct_mapping);

    Ok((cache, config))
}

pub(crate) async fn run(schema: String, config: Config) -> Result<(), Error> {
    let kratos = Configuration {
        base_path: config.kratos_url.to_string(),
        ..Default::default()
    };

    let (_, config) = fetch(&kratos, &config.keyword, &schema, config.direct_mapping).await?;

    // TODO: beautify output
    println!("{config:#?}");

    Ok(())
}
