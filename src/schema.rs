use std::ops::Deref;

use error_stack::Result;
use indexmap::IndexMap;
use jsonptr::Token;
use schemars::schema::{ObjectValidation, Schema, SchemaObject};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

const KEYWORD: &str = "indietyp/consent";

#[derive(Debug)]
pub(crate) struct ImplicitScopeCache(IndexMap<Scope, Vec<jsonptr::Pointer>>);

impl ImplicitScopeCache {
    fn new() -> Self {
        Self(IndexMap::new())
    }

    fn get(&self, scope: &Scope) -> Option<&Vec<jsonptr::Pointer>> {
        self.0.get(scope)
    }

    fn merge(&mut self, other: Self) {
        for (scope, pointers) in other.0 {
            self.0.entry(scope).or_default().extend(pointers);
        }
    }

    fn insert(&mut self, scope: Scope, pointer: jsonptr::Pointer) {
        self.0.entry(scope).or_default().push(pointer);
    }
}

#[derive(Debug)]
pub(crate) struct Cache {
    implicit_scopes: ImplicitScopeCache,
}

impl Cache {
    pub(crate) fn new(implicit_scopes: ImplicitScopeCache) -> Self {
        Self { implicit_scopes }
    }
}

pub(crate) struct Claims {
    pub(crate) id_token: Value,
    pub(crate) access_token: Value,
}

// A claim is a resolved scope with a value.
pub(crate) struct Claim<'a> {
    scope: &'a Scope,
    value: Value,
    session_data: &'a SessionData,
}

struct IncompleteClaim<'a> {
    value: Value,
    session_data: &'a SessionData,
}

impl<'a> IncompleteClaim<'a> {
    fn complete(self, scope: &'a Scope) -> Claim<'a> {
        Claim {
            scope,
            value: self.value,
            session_data: self.session_data,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(crate) struct SessionData {
    pub(crate) id_token: Option<String>,
    pub(crate) access_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(crate) struct Scope(String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(crate) struct TraitConfiguration {
    pub(crate) scopes: Vec<Scope>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Collect {
    First,
    Last,
    Any,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(crate) struct ImplicitScope {
    collect: Collect,
    session_data: SessionData,
}

impl ImplicitScope {
    fn find_object(object: ObjectValidation, path: Vec<Token>) -> ImplicitScopeCache {
        let mut pointers = ImplicitScopeCache::new();

        for (key, value) in object.properties {
            let mut path = path.clone();

            path.push(Token::new(key));

            pointers.merge(Self::find(value.into_object(), path));
        }

        pointers
    }

    // This is not ideal, ideally we'd go through the user object (with schema in hand) and evaluate
    // the schema for every entry. However, this is a lot of work and we're not sure if it's worth
    // for a PoC. (also: I didn't find a way to do this with any of the existing crates)
    pub(crate) fn find(mut schema: SchemaObject, path: Vec<Token>) -> ImplicitScopeCache {
        let mut pointers = ImplicitScopeCache::new();

        if let Some(object) = schema.object {
            pointers.merge(Self::find_object(*object, path.clone()));
        }

        if let Some(extension) = schema.extensions.remove(KEYWORD) {
            let pointer = jsonptr::Pointer::new(path);

            match serde_json::from_value::<TraitConfiguration>(extension.clone()) {
                Ok(value) => {
                    for scope in value.scopes {
                        pointers.insert(scope, pointer.clone());
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        ?error,
                        ?extension,
                        "unable to deserialize trait configuration"
                    );
                }
            }
        }

        pointers
    }

    fn resolve<'a>(&'a self, scope: &Scope, traits: &Value, cache: &Cache) -> IncompleteClaim<'a> {
        let Some(pointers) = cache.implicit_scopes.get(scope) else {
            tracing::warn!("unable to find scope in cache");

            return IncompleteClaim {
                value: Value::Null,
                session_data: &self.session_data,
            }
        };

        let mut values = vec![];

        for pointer in pointers {
            match pointer.resolve(traits) {
                Ok(value) => {
                    values.push(value);
                }
                Err(error) => {
                    tracing::warn!(?error, ?pointer, "unable to resolve pointer");
                }
            }
        }

        let value = match self.collect {
            Collect::Any | Collect::First => {
                values.into_iter().next().cloned().unwrap_or(Value::Null)
            }
            Collect::Last => values.pop().cloned().unwrap_or(Value::Null),
            Collect::All => values.into_iter().cloned().collect(),
        };

        IncompleteClaim {
            value,
            session_data: &self.session_data,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Pointer(jsonptr::Pointer);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum ScopeExplicitMapping {
    Object {
        properties: IndexMap<String, ScopeExplicitMapping>,
    },
    Tuple {
        #[serde(rename = "prefixItems")]
        items: Vec<ScopeExplicitMapping>,
    },
    Path {
        #[serde(rename = "$ref")]
        ref_: Pointer,
    },
}

impl ScopeExplicitMapping {
    fn resolve(&self, scope: &Scope, value: &Value) -> Value {
        match self {
            ScopeExplicitMapping::Object { properties } => {
                let mut object = serde_json::Map::new();

                for (key, mapping) in properties {
                    object.insert(key.clone(), mapping.resolve(scope, value));
                }

                Value::Object(object)
            }
            ScopeExplicitMapping::Tuple { items } => {
                let mut array = Vec::with_capacity(items.len());

                for mapping in items {
                    array.push(mapping.resolve(scope, value));
                }

                Value::Array(array)
            }
            ScopeExplicitMapping::Path { ref_ } => {
                let pointer = &ref_.0;

                match pointer.resolve(value) {
                    Ok(value) => value.clone(),
                    Err(error) => {
                        tracing::warn!(?error, ?pointer, "unable to resolve pointer");

                        Value::Null
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ExplicitScope {
    mapping: ScopeExplicitMapping,
    session_data: SessionData,
}

impl ExplicitScope {
    fn resolve<'a>(&'a self, scope: &Scope, traits: &Value) -> IncompleteClaim<'a> {
        let value = self.mapping.resolve(scope, traits);

        IncompleteClaim {
            value,
            session_data: &self.session_data,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum ScopeConfiguration {
    Implicit(ImplicitScope),
    Explicit(ExplicitScope),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Configuration {
    pub(crate) scopes: IndexMap<Scope, ScopeConfiguration>,
}

impl Configuration {
    fn empty() -> Self {
        Self {
            scopes: IndexMap::new(),
        }
    }

    pub(crate) fn find_scope(&self, scope: &Scope) -> Option<&ScopeConfiguration> {
        self.scopes.get(scope)
    }

    pub(crate) fn resolve<'a>(
        &'a self,
        scope: &'a Scope,
        traits: &Value,
        cache: &Cache,
    ) -> Option<Claim<'a>> {
        let mapping = self.find_scope(scope)?;

        let claim = match mapping {
            ScopeConfiguration::Implicit(implicit) => implicit.resolve(scope, traits, cache),
            ScopeConfiguration::Explicit(explicit) => explicit.resolve(scope, traits),
        }
        .complete(scope);

        Some(claim)
    }

    pub(crate) fn resolve_all(&self, traits: &Value, cache: &Cache) -> Claims {
        let mut claims = vec![];

        for scope in self.scopes.keys() {
            if let Some(claim) = self.resolve(scope, traits, cache) {
                claims.push(claim);
            }
        }

        let id_token = claims
            .iter()
            .filter_map(|claim| {
                claim
                    .session_data
                    .id_token
                    .clone()
                    .map(|id_token| (id_token, claim.value.clone()))
            })
            .collect();

        let access_token = claims
            .into_iter()
            .filter_map(|claim| {
                claim
                    .session_data
                    .access_token
                    .clone()
                    .map(|access_token| (access_token, claim.value))
            })
            .collect();

        Claims {
            id_token: Value::Object(id_token),
            access_token: Value::Object(access_token),
        }
    }

    // search for all scopes that are not explicitly defined and create an implicit mapping for them
    // we do not overwrite existing mappings
    fn insert_implicit_mapping(&mut self, cache: &Cache) {
        // we have already gathered all scopes that have been defined (through the cache), diff
        // which ones are missing.

        for scope in cache.implicit_scopes.0.keys() {
            if self.scopes.contains_key(scope) {
                continue;
            }

            let mapping = ScopeConfiguration::Implicit(ImplicitScope {
                collect: Collect::First,
                session_data: SessionData {
                    id_token: Some(scope.0.clone()),
                    access_token: Some(scope.0.clone()),
                },
            });

            self.scopes.insert(scope.clone(), mapping);
        }
    }

    // direct mappings are automatic mappings for the first level of the object
    // we do not overwrite existing mappings
    fn insert_direct_mapping(&mut self, value: &SchemaObject) {
        let Some(object) = &value.object else {
            return;
        };

        for key in object.properties.keys() {
            let scope = Scope(key.clone());

            if self.scopes.contains_key(&scope) {
                continue;
            }

            let mapping = ScopeConfiguration::Implicit(ImplicitScope {
                collect: Collect::First,
                session_data: SessionData {
                    id_token: Some(key.clone()),
                    access_token: Some(key.clone()),
                },
            });

            self.scopes.insert(scope, mapping);
        }
    }

    pub(crate) fn from_root(mut schema: SchemaObject, cache: &Cache, direct_mapping: bool) -> Self {
        let Some(value) = schema.extensions.remove(KEYWORD) else {
            tracing::warn!("unable to find {KEYWORD} in identity schema");

            return Self::empty();
        };

        let Ok(mut configuration) = serde_json::from_value::<Self>(value) else {
            tracing::warn!("unable to deserialize {KEYWORD} in identity schema");

            return Self::empty();
        };

        configuration.insert_implicit_mapping(cache);
        if direct_mapping {
            configuration.insert_direct_mapping(&schema);
        }

        configuration
    }
}
