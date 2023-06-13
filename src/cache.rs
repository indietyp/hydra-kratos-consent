use std::sync::Arc;

use error_stack::Result;
use indexmap::IndexMap;
use ory_kratos_client::apis::configuration::Configuration;
use serde_json::Value;
use tokio::sync::RwLock;

use crate::{
    schema::{Claims, Scope, ScopeConfig},
    validate::{fetch, Error},
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SchemaId(String);

impl SchemaId {
    pub(crate) const fn new(value: String) -> Self {
        Self(value)
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImplicitScopeCache(IndexMap<Scope, Vec<jsonptr::Pointer>>);

impl ImplicitScopeCache {
    pub(crate) fn new() -> Self {
        Self(IndexMap::new())
    }

    pub(crate) fn get(&self, scope: &Scope) -> Option<&Vec<jsonptr::Pointer>> {
        self.0.get(scope)
    }

    pub(crate) fn merge(&mut self, other: Self) {
        for (scope, pointers) in other.0 {
            self.0.entry(scope).or_default().extend(pointers);
        }
    }

    pub(crate) fn insert(&mut self, scope: Scope, pointer: jsonptr::Pointer) {
        self.0.entry(scope).or_default().push(pointer);
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = &Scope> {
        self.0.keys()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScopeCache {
    pub(crate) implicit_scopes: ImplicitScopeCache,
}

impl ScopeCache {
    pub(crate) const fn new(implicit_scopes: ImplicitScopeCache) -> Self {
        Self { implicit_scopes }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Schema {
    cache: ScopeCache,

    config: ScopeConfig,
}

impl Schema {
    pub(crate) fn resolve(&self, traits: &Value, requested: &[Scope]) -> Claims {
        self.config.resolve_all(traits, &self.cache, requested)
    }
}

#[derive(Debug)]
pub(crate) struct SchemaCache {
    direct_mapping: bool,
    keyword: String,
    data: RwLock<IndexMap<SchemaId, Arc<Schema>>>,
}

impl SchemaCache {
    pub(crate) fn new(keyword: String, direct_mapping: bool) -> Self {
        Self {
            keyword,
            data: RwLock::new(IndexMap::new()),
            direct_mapping,
        }
    }

    async fn insert(&self, id: SchemaId, schema: Schema) {
        let mut lock = self.data.write().await;

        lock.insert(id, Arc::new(schema));
    }

    async fn contains_key(&self, id: &SchemaId) -> bool {
        let lock = self.data.read().await;

        lock.contains_key(id)
    }

    async fn get(&self, id: &SchemaId) -> Option<Arc<Schema>> {
        let lock = self.data.read().await;

        lock.get(id).map(Arc::clone)
    }

    async fn get_or_panic(&self, id: &SchemaId) -> Arc<Schema> {
        let lock = self.data.read().await;

        Arc::clone(&lock[id])
    }

    pub(crate) async fn fetch(
        &self,
        config: &Configuration,
        id: &SchemaId,
    ) -> Result<Arc<Schema>, Error> {
        if self.contains_key(id).await {
            return Ok(self.get_or_panic(id).await);
        }

        let (cache, config) =
            fetch(config, &self.keyword, id.as_str(), self.direct_mapping).await?;

        self.insert(id.clone(), Schema { cache, config }).await;

        Ok(self.get_or_panic(id).await)
    }
}
