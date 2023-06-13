use std::{
    collections::HashSet,
    fmt::{Display, Formatter},
};

use indexmap::IndexMap;
use jsonptr::Token;
use schemars::schema::{ObjectValidation, SchemaObject};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cache::{ImplicitScopeCache, ScopeCache};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(crate) struct Scope(String);

impl Scope {
    pub(crate) fn new(value: String) -> Self {
        Self(value)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
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
    #[allow(clippy::missing_const_for_fn)] // Reason: false positive
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
    fn find_object(keyword: &str, object: ObjectValidation, path: &[Token]) -> ImplicitScopeCache {
        let mut pointers = ImplicitScopeCache::new();

        for (key, value) in object.properties {
            let mut path = path.to_vec();

            path.push(Token::new(key));

            pointers.merge(Self::find(keyword, value.into_object(), path));
        }

        pointers
    }

    // This is not ideal, ideally we'd go through the user object (with schema in hand) and evaluate
    // the schema for every entry. However, this is a lot of work and we're not sure if it's worth
    // for a PoC. (also: I didn't find a way to do this with any of the existing crates)
    pub(crate) fn find(
        keyword: &str,
        mut schema: SchemaObject,
        path: Vec<Token>,
    ) -> ImplicitScopeCache {
        let mut pointers = ImplicitScopeCache::new();

        if let Some(object) = schema.object {
            pointers.merge(Self::find_object(keyword, *object, &path));
        }

        if let Some(extension) = schema.extensions.remove(keyword) {
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

    fn resolve<'a>(
        &'a self,
        scope: &Scope,
        traits: &Value,
        cache: &ScopeCache,
    ) -> IncompleteClaim<'a> {
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

impl Display for Pointer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

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
    fn resolve(&self, value: &Value) -> Value {
        match self {
            Self::Object { properties } => {
                let mut object = serde_json::Map::new();

                for (key, mapping) in properties {
                    object.insert(key.clone(), mapping.resolve(value));
                }

                Value::Object(object)
            }
            Self::Tuple { items } => {
                let mut array = Vec::with_capacity(items.len());

                for mapping in items {
                    array.push(mapping.resolve(value));
                }

                Value::Array(array)
            }
            Self::Path { ref_ } => {
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
    fn resolve(&self, traits: &Value) -> IncompleteClaim {
        let value = self.mapping.resolve(traits);

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
pub(crate) struct ScopeConfig {
    pub(crate) scopes: IndexMap<Scope, ScopeConfiguration>,
}

impl ScopeConfig {
    fn empty() -> Self {
        Self {
            scopes: IndexMap::new(),
        }
    }

    pub(crate) fn find_scope(&self, scope: &Scope) -> Option<&ScopeConfiguration> {
        self.scopes.get(scope)
    }

    #[tracing::instrument]
    pub(crate) fn resolve<'a>(
        &'a self,
        scope: &'a Scope,
        traits: &Value,
        cache: &ScopeCache,
    ) -> Option<Claim<'a>> {
        let mapping = self.find_scope(scope)?;

        let claim = match mapping {
            ScopeConfiguration::Implicit(implicit) => {
                tracing::debug!(?scope, "resolving implicit scope");

                implicit.resolve(scope, traits, cache)
            }
            ScopeConfiguration::Explicit(explicit) => {
                tracing::debug!(?scope, "resolving explicit scope");

                explicit.resolve(traits)
            }
        }
        .complete(scope);

        Some(claim)
    }

    #[tracing::instrument]
    pub(crate) fn resolve_all(
        &self,
        traits: &Value,
        cache: &ScopeCache,
        requested: &HashSet<Scope>,
    ) -> Claims {
        let mut claims = vec![];

        for scope in self.scopes.keys() {
            if !requested.contains(scope) {
                continue;
            }

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
    fn insert_implicit_mapping(&mut self, cache: &ScopeCache) {
        // we have already gathered all scopes that have been defined (through the cache), diff
        // which ones are missing.

        for scope in cache.implicit_scopes.keys() {
            if self.scopes.contains_key(scope) {
                continue;
            }

            let mapping = ScopeConfiguration::Implicit(ImplicitScope {
                collect: Collect::First,
                session_data: SessionData {
                    id_token: Some(scope.as_str().to_owned()),
                    access_token: Some(scope.as_str().to_owned()),
                },
            });

            self.scopes.insert(scope.clone(), mapping);
        }
    }

    // direct mappings are automatic mappings for the first level of the object
    // we do not overwrite existing mappings
    fn insert_direct_mapping(&mut self, value: &SchemaObject, cache: &mut ScopeCache) {
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

            self.scopes.insert(scope.clone(), mapping);

            cache
                .implicit_scopes
                .insert(scope, jsonptr::Pointer::from(Token::new(key)));
        }
    }

    fn create(keyword: &str, schema: &mut SchemaObject) -> Self {
        let Some(value) = schema.extensions.remove(keyword) else {
            tracing::warn!("unable to find {keyword} in identity schema");

            return Self::empty();
        };

        match serde_json::from_value::<Self>(value) {
            Ok(this) => this,
            Err(error) => {
                tracing::warn!(?error, "unable to deserialize {keyword} in identity schema");

                Self::empty()
            }
        }
    }

    pub(crate) fn from_root(
        keyword: &str,
        mut schema: SchemaObject,
        cache: &mut ScopeCache,
        direct_mapping: bool,
    ) -> Self {
        let mut this = Self::create(keyword, &mut schema);

        this.insert_implicit_mapping(cache);
        if direct_mapping {
            this.insert_direct_mapping(&schema, cache);
        }

        this
    }
}
