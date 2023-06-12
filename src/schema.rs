use error_stack::Result;
use indexmap::IndexMap;
use jsonschema::JSONSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub(crate) struct Cache {
    implicit_scopes: IndexMap<Scope, Vec<jsonptr::Pointer>>,
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
    fn find(&self, schema: &JSONSchema) -> Vec<jsonptr::Pointer> {
        // Problem: we're currently unable to traverse the schema
        // TODO: this won't work because of definitions and such, we would need to keep track where
        // we are, then resolve that in the schema and figure out if we allow it or not.
        todo!()
    }

    fn resolve(&self, scope: &Scope, user: &Value, cache: &Cache) -> IncompleteClaim {
        // go through the schema and find all the scopes that match
        // then collect the values from the user

        let Some(pointers) = cache.implicit_scopes.get(scope) else {
            tracing::warn!("unable to find scope in cache");

            return IncompleteClaim {
                value: Value::Null,
                session_data: &self.session_data,
            }
        };

        let mut values = vec![];

        for pointer in pointers {
            match pointer.resolve(user) {
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
    fn resolve(&self, scope: &Scope, user: &Value) -> IncompleteClaim {
        let value = self.mapping.resolve(scope, user);

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
    fn find_scope(&self, scope: &Scope) -> Option<&ScopeConfiguration> {
        self.scopes.get(scope)
    }

    fn resolve(&self, scope: &Scope, user: &Value, cache: &Cache) -> Claim {
        let mapping = self.find_scope(scope)?;

        match mapping {
            ScopeConfiguration::Implicit(implicit) => implicit.resolve(scope, user, cache),
            ScopeConfiguration::Explicit(explicit) => explicit.resolve(scope, user),
        }
        .complete(scope)
    }
}
