# hydra-kratos-consent

hydra-kratos-consent is a [Hydra](https://www.ory.sh/hydra/docs/) consent provider
for [Kratos](https://www.ory.sh/kratos/docs/).

It aims to be highly customizable and easy to use.

Currently only `skip_consent` is supported, plans for the future include adding the ability for consent.

## Usage

Simply start the server with `./hydra-kratos-consent serve <host>:<port>` and configure Hydra to use it as a consent
provider.

You can validate your schema using `./hydra-kratos-consent validate`.

### Configuration

The following environment variables are supported:

| Name               | Description                                                         | Default                |
|--------------------|---------------------------------------------------------------------|------------------------|
| `HYDRA_ADMIN_URL`  | The URL of the Hydra server                                         | -                      |
| `KRATOS_ADMIN_URL` | The URL of the Kratos server                                        | -                      |
| `BASE_URL`         | The base URL of the server (without `/consent`), used for redirects | `http://<host>:<port>` |
| `DIRECT_MAPPING`   | Whether to enable direct mappings                                   | `false`                |
| `SKIP_CONSENT`     | Whether to skip consent, currently no way to disable                | `true`                 |
| `RUST_LOG`         | The log level                                                       | `info`                 |

### Configuration in Identity Schema

Claims that are to be used for claims, can additionally be marked in the identity schema.

To do so, the following extensions to the schema are provided:

#### Traits

Every property in the `traits` object can have an additional `indietyp/consent` property of the following schema:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "trait-consent",
  "title": "Trait Consent",
  "type": "object",
  "properties": {
    "scopes": {
      "type": "array",
      "items": {
        "type": "string"
      }
    }
  },
  "required": [
    "scopes"
  ]
}
```

These are then configured in the top schema, if `DIRECT_MAPPING` is enabled, an entry will automatically be generated
for every property in the `traits` object.

##### Limitations

Implicit configuration currently _only_ works in objects that are:

1) directly embedded (we do not follow references)
2) not conditional (we do not follow `if`/`then`/`else`)
3) are either contained in another object or in the `traits` object

This is due to the fact that internally we first resolve the schema into a list of json pointers, and then resolve those
on the object. (contrary to json paths, which are not standardized, we do not allow for wildcards, this may change in
the future)

#### Consent Configuration

Invalid configurations will be ignored on consent, but will emit a warning. You can check the validity of your schema
using `./hydra-kratos-consent validate`.

Scopes are configured in the top level of the schema, using the `indietype/consent` property, if they are not mentioned,
it is presumed that they default to:

```json
{
  "type": "implicit",
  "sessionData": {
    "idToken": "<name of the scope>",
    "accessToken": "<name of the scope>"
  },
  "collect": "first"
}
```

```json5
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "consent-configuration",
  "title": "Configuration of Scopes",
  "type": "object",
  "properties": {
    "scopes": {
      "type": "object",
      "additionalProperties": {
        "$ref": "#/definitions/scopes"
      }
    }
  },
  "definitions": {
    "sessionData": {
      "type": "object",
      "properties": {
        "idToken": {
          "type": "string"
        },
        "accessToken": {
          "type": "string"
        }
      },
      "$anyOf": [
        {
          "required": [
            "idToken"
          ]
        },
        {
          "required": [
            "accessToken"
          ]
        }
      ],
      "additionalProperties": false
    }
  },
  "scopes": {
    "$oneOf": [
      {
        "$ref": "#/definitions/scope-implicit"
      },
      {
        "$ref": "#/definitions/scope-explicit"
      }
    ]
  },
  "scope-implicit": {
    "type": "object",
    "properties": {
      "type": {
        "type": "string",
        "const": "implicit"
      },
      "collect": {
        "type": "string",
        "enum": [
          "first",
          "last",
          "any",
          "all"
        ],
        "default": "first"
      },
      "sessionData": {
        "$ref": "#/definitions/sessionData"
      }
    },
    "required": [
      "sessionData",
      "type"
    ]
  },
  "json-pointer": {
    // a valid JSON path pointing to a value
    "type": "object",
    "properties": {
      "$ref": {
        "type": "string"
      },
    },
    "required": [
      "$ref"
    ]
  },
  "scope-mapping-object": {
    "type": "object",
    "properties": {
      "type": {
        "type": "string",
        "const": "object"
      },
      "properties": {
        "type": "object",
        "additionalProperties": {
          "$ref": "#/definitions/scope-mapping"
        }
      }
    },
    "required": [
      "type",
      "properties"
    ]
  },
  "scope-mapping-tuple": {
    "type": "object",
    "properties": {
      "type": {
        "type": "string",
        "const": "tuple"
      },
      "prefixItems": {
        "type": "array",
        "items": {
          "$ref": "#/definitions/scope-mapping"
        }
      }
    },
    "required": [
      "type",
      "items"
    ]
  },
  "scope-mapping": {
    "$oneOf": [
      {
        "$ref": "#/definitions/scope-mapping-object"
      },
      {
        "$ref": "#/definitions/scope-mapping-tuple"
      },
      {
        "$ref": "#/definitions/json-pointer"
      }
    ]
  },
  "scope-explicit": {
    "type": "object",
    "properties": {
      "type": {
        "type": "string",
        "const": "explicit"
      },
      "mapping": {
        "$ref": "#/definitions/scope-mapping"
      },
      "sessionData": {
        "$ref": "#/definitions/sessionData"
      }
    }
  }
}
```

##### Example

```json5
{
  /* "traits": ... */
  "indietyp/consent": {
    "mapping": {
      "type": "explicit",
      "mapping": {
        "type": "object",
        "properties": {
          "email": {
            "$ref": "#/email"
          },
          "name": {
            "$ref": "#/name"
          }
        }
      },
      "sessionData": {
        "idToken": "profile",
        "accessToken": "profile"
      }
    }
  }
}
```

## Future Possibilities

- [ ] Support for remote content
