# Agenterra Configuration ⚙️

This guide explains how to configure Agenterra using different methods.

## Table of Contents
- [Configuration Methods](#configuration-methods)
- [Command-Line Options](#command-line-options)
- [Configuration File](#configuration-file)
- [Environment Variables](#environment-variables)
- [Example Configurations](#example-configurations)

## Configuration Methods

Agenterra can be configured using the following methods (in order of precedence):

1. **Command-Line Arguments** (highest priority)
2. **Configuration File** (`agenterra.toml` in project root)
3. **Environment Variables**
4. **Default Values** (lowest priority)

## Command-Line Options

### Global Options

```bash
agenterra [OPTIONS] <SUBCOMMAND>
```

| Option | Description | Default |
|--------|-------------|---------|
| `-h`, `--help` | Print help | |
| `-V`, `--version` | Print version | |

### Scaffold MCP Server

```bash
agenterra scaffold mcp server --schema-path <SCHEMA_PATH> [OPTIONS]
```

| Option | Description | Default |
|--------|-------------|---------|
| `--schema-path <SCHEMA_PATH>` | Path or URL to OpenAPI schema (YAML or JSON) | *required* |
| `--project-name <PROJECT_NAME>` | Project name | `agenterra_mcp_server` |
| `--template <TEMPLATE>` | Template to use for code generation | `rust_axum` |
| `--template-dir <TEMPLATE_DIR>` | Custom template directory | |
| `--output-dir <OUTPUT_DIR>` | Output directory for generated code | |
| `--log-file <LOG_FILE>` | Log file name without extension | `mcp-server` |
| `--port <PORT>` | Server port | `3000` |
| `--base-url <BASE_URL>` | Base URL of the OpenAPI specification | |

### Scaffold MCP Client

```bash
agenterra scaffold mcp client --project-name <PROJECT_NAME> [OPTIONS]
```

| Option | Description | Default |
|--------|-------------|---------|
| `--project-name <PROJECT_NAME>` | Project name | `agenterra_mcp_client` |
| `--template <TEMPLATE>` | Template to use for code generation | `rust_reqwest` |
| `--template-dir <TEMPLATE_DIR>` | Custom template directory | |
| `--output-dir <OUTPUT_DIR>` | Output directory for generated code | |
| `--timeout <TIMEOUT>` | Connection timeout in seconds | `10` |

## Configuration File

Create a `agenterra.toml` file in your project root:

### Server Configuration

```toml
[scaffold.mcp.server]
schema_path = "openapi.yaml"
project_name = "my_api_server"
template = "rust_axum"
output_dir = "generated-server"
log_file = "my-server"
port = 3000
base_url = "https://api.example.com"

# Custom template directory (optional)
template_dir = "./custom-templates/server"
```

### Client Configuration

```toml
[scaffold.mcp.client]
project_name = "my_api_client"
template = "rust_reqwest"
output_dir = "generated-client"
timeout = 10

# Custom template directory (optional)
template_dir = "./custom-templates/client"
```

### Combined Configuration

```toml
# Server configuration
[scaffold.mcp.server]
schema_path = "api/openapi.yaml"
project_name = "petstore_mcp_server"
template = "rust_axum"
output_dir = "petstore-server"
log_file = "petstore-server"
port = 3000
base_url = "https://petstore3.swagger.io"

# Client configuration
[scaffold.mcp.client]
project_name = "petstore_mcp_client"
template = "rust_reqwest"
output_dir = "petstore-client"
timeout = 30
```

## Environment Variables

Configuration options can be set via environment variables with the `AGENTERRA_` prefix:

### Server Environment Variables

```bash
# Basic options
export AGENTERRA_SCHEMA_PATH=openapi.yaml
export AGENTERRA_OUTPUT_DIR=generated-server
export AGENTERRA_PROJECT_NAME=my_api_server

# Template options
export AGENTERRA_TEMPLATE=rust_axum
export AGENTERRA_TEMPLATE_DIR=./custom-templates/server

# Server options
export AGENTERRA_PORT=8080
export AGENTERRA_BASE_URL=https://api.example.com
export AGENTERRA_LOG_FILE=my-server
```

### Client Environment Variables

```bash
# Basic options
export AGENTERRA_OUTPUT_DIR=generated-client
export AGENTERRA_PROJECT_NAME=my_api_client

# Template options
export AGENTERRA_TEMPLATE=rust_reqwest
export AGENTERRA_TEMPLATE_DIR=./custom-templates/client

# Client options
export AGENTERRA_TIMEOUT=30
```

## Example Configurations

### Minimal Server Configuration

```toml
[scaffold.mcp.server]
schema_path = "api/openapi.yaml"
output_dir = "generated-server"
```

### Minimal Client Configuration

```toml
[scaffold.mcp.client]
project_name = "my-client"
output_dir = "generated-client"
```

### Full Server Configuration

```toml
[scaffold.mcp.server]
schema_path = "api/openapi.yaml"
project_name = "petstore_mcp_server"
template = "rust_axum"
output_dir = "petstore-server"
log_file = "petstore-server"
port = 3000
base_url = "https://petstore3.swagger.io"
```

### Full Client Configuration

```toml
[scaffold.mcp.client]
project_name = "petstore_mcp_client"
template = "rust_reqwest"
output_dir = "petstore-client"
timeout = 30
```

### Environment Variables Example

```bash
# .env file for server generation
AGENTERRA_SCHEMA_PATH=api/openapi.yaml
AGENTERRA_OUTPUT_DIR=generated-server
AGENTERRA_PROJECT_NAME=my_api_server
AGENTERRA_TEMPLATE=rust_axum
AGENTERRA_PORT=3000
AGENTERRA_BASE_URL=https://api.example.com

# .env file for client generation
AGENTERRA_OUTPUT_DIR=generated-client
AGENTERRA_PROJECT_NAME=my_api_client
AGENTERRA_TEMPLATE=rust_reqwest
AGENTERRA_TIMEOUT=30
```

## Configuration Precedence

1. Command-line arguments (highest priority)
2. Environment variables
3. Configuration file (`agenterra.toml`)
4. Default values (lowest priority)

## Migration from Previous Versions

If you're upgrading from a previous version, update your configuration files:

**Old format (deprecated):**
```toml
[scaffold]
schema_path = "openapi.yaml"
template_kind = "rust_axum"
output_dir = "generated"
```

**New format:**
```toml
[scaffold.mcp.server]
schema_path = "openapi.yaml"
template = "rust_axum"
output_dir = "generated-server"

[scaffold.mcp.client]
project_name = "my-client"
template = "rust_reqwest"
output_dir = "generated-client"
```

## Next Steps

- [CLI Reference](CLI_REFERENCE.md)
- [Templates Documentation](TEMPLATES.md)
- [Contributing Guide](../CONTRIBUTING.md)