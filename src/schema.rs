use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(crate) struct SessionData {
    pub(crate) id_token: Option<String>,
    pub(crate) access_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(crate) struct Scope(String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(crate) struct TraitConfig {
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
pub(crate) struct ScopeValueMapping {
    collect: Collect,
    session_data: SessionData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Pointer(jsonptr::Pointer);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum ScopeComplexMapping {
    Object {
        properties: IndexMap<String, ScopeComplexMapping>,
    },
    Tuple {
        #[serde(rename = "prefixItems")]
        items: Vec<ScopeComplexMapping>,
    },
    Path {
        #[serde(rename = "$ref")]
        ref_: Pointer,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum ScopeMapping {
    Value(ScopeValueMapping),
    Complex(ScopeComplexMapping),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Configuration {
    pub(crate) scopes: IndexMap<Scope, ScopeMapping>,
}
