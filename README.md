# hydra-kratos-consent

hydra-kratos-consent is a [Hydra](https://www.ory.sh/hydra/docs/) consent provider
for [Kratos](https://www.ory.sh/kratos/docs/).

It aims to be highly customizable and easy to use.

Currently only `skip_consent` is supported, plans for the future include adding the ability for consent.

## Usage

Simply start the server with `./hydra-kratos-consent <host>:<port>` and configure Hydra to use it as a consent provider.

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

