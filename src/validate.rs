use std::io::Write;

use console::Term;
use error_stack::{IntoReport, Result, ResultExt};
use ory_kratos_client::apis::configuration::Configuration;
use ron_to_table::RonTable;
use schemars::schema::SchemaObject;
use serde::Deserialize;
use tabled::settings::Style;
use thiserror::Error;

use crate::{cache::ScopeCache, schema::ImplicitScope, serve::Config};

#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error("error while fetching from Kratos")]
    Kratos,
    #[error("schema is malformed")]
    IdentitySchemaMalformed,
    #[error("unable to deserialize schema")]
    Serde,
    #[error("unable to write to stdout")]
    Io,
}

pub(crate) async fn fetch(
    config: &Configuration,
    keyword: &str,
    id: &str,
    direct_mapping: bool,
) -> Result<(ScopeCache, crate::schema::ScopeConfig), Error> {
    // fetch the identity schema from kratos
    let identity_schema = ory_kratos_client::apis::identity_api::get_identity_schema(config, id)
        .await
        .into_report()
        .change_context(Error::Kratos)?;

    let traits = identity_schema
        .get("properties")
        .ok_or_else(|| {
            tracing::error!("identity schema is malformed");
            Error::IdentitySchemaMalformed
        })?
        .get("traits")
        .ok_or_else(|| {
            tracing::error!("identity schema is malformed");
            Error::IdentitySchemaMalformed
        })?
        .clone();

    let schema: SchemaObject = serde_json::from_value(traits)
        .into_report()
        .change_context(Error::IdentitySchemaMalformed)?;

    tracing::debug!(?schema, "fetched schema from kratos");

    let cache = ImplicitScope::find(keyword, schema.clone(), vec![]);
    let cache = ScopeCache::new(cache);

    let config = crate::schema::ScopeConfig::from_root(keyword, schema, &cache, direct_mapping);

    Ok((cache, config))
}

pub(crate) async fn run(schema: String, config: Config) -> Result<(), Error> {
    let kratos = Configuration {
        base_path: config.kratos_url.as_str().trim_end_matches('/').to_owned(),
        ..Default::default()
    };

    let (_, config) = fetch(&kratos, &config.keyword, &schema, config.direct_mapping).await?;

    let config = serde_value::to_value(config)
        .into_report()
        .change_context(Error::Kratos)?;

    let config: ron::Value = ron::Value::deserialize(config)
        .into_report()
        .change_context(Error::Serde)?;

    let table = RonTable::new()
        .collapse()
        .with(Style::rounded())
        .build(&config);

    let mut term = Term::stdout();
    term.write_all(table.as_bytes())
        .into_report()
        .change_context(Error::Io)?;

    Ok(())
}
