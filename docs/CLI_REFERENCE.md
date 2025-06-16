# Agenterra CLI Reference 📝

This document provides a comprehensive reference for the Agenterra command-line interface.

## Table of Contents
- [Global Options](#global-options)
- [Commands](#commands)
  - [scaffold](#scaffold)
    - [scaffold mcp](#scaffold-mcp)
      - [scaffold mcp server](#scaffold-mcp-server)
      - [scaffold mcp client](#scaffold-mcp-client)
- [Examples](#examples)
- [Exit Codes](#exit-codes)
- [Environment Variables](#environment-variables)

## Global Options

| Option | Description |
|--------|-------------|
| `-h`, `--help` | Print help |
| `-V`, `--version` | Print version |

## Commands

### scaffold

Scaffold MCP servers and clients from specifications using the universal taxonomy.

```bash
agenterra scaffold <PROTOCOL> <ROLE> [OPTIONS]
```

#### scaffold mcp

Generate MCP (Model Context Protocol) servers and clients.

```bash
agenterra scaffold mcp <ROLE> [OPTIONS]
```

##### scaffold mcp server

Generate an MCP server from an OpenAPI specification.

```bash
agenterra scaffold mcp server --schema-path <SCHEMA_PATH> [OPTIONS]
```

**Options:**

| Option | Description | Default |
|--------|-------------|---------|
| `--schema-path <SCHEMA_PATH>` | Path or URL to OpenAPI schema (YAML or JSON). Can be a local file path or an HTTP/HTTPS URL. | *required* |
| `--project-name <PROJECT_NAME>` | Project name | `agenterra_mcp_server` |
| `--template <TEMPLATE>` | Template to use for code generation | `rust_axum` |
| `--template-dir <TEMPLATE_DIR>` | Custom template directory (only used with --template=custom) | |
| `--output-dir <OUTPUT_DIR>` | Output directory for generated code | |
| `--log-file <LOG_FILE>` | Log file name without extension | `mcp-server` |
| `--port <PORT>` | Server port | `3000` |
| `--base-url <BASE_URL>` | Base URL of the OpenAPI specification (Optional) | |

**Available Server Templates:**
- `rust_axum` - Rust MCP server using Axum web framework (default)

##### scaffold mcp client

Generate an MCP client for connecting to MCP servers.

```bash
agenterra scaffold mcp client --project-name <PROJECT_NAME> [OPTIONS]
```

**Options:**

| Option | Description | Default |
|--------|-------------|---------|
| `--project-name <PROJECT_NAME>` | Project name | `agenterra_mcp_client` |
| `--template <TEMPLATE>` | Template to use for code generation | `rust_reqwest` |
| `--template-dir <TEMPLATE_DIR>` | Custom template directory (only used with --template=custom) | |
| `--output-dir <OUTPUT_DIR>` | Output directory for generated code | |
| `--timeout <TIMEOUT>` | Connection timeout in seconds | `10` |

**Available Client Templates:**
- `rust_reqwest` - Rust MCP client with REPL interface (default)

## Examples

### Server Generation

```bash
# Basic server generation from local file
agenterra scaffold mcp server --schema-path api.yaml --output-dir generated-server

# Server from remote OpenAPI spec
agenterra scaffold mcp server --schema-path https://petstore3.swagger.io/api/v3/openapi.json --output-dir petstore-server --base-url https://petstore3.swagger.io

# Custom project name and template
agenterra scaffold mcp server --schema-path api.yaml --output-dir my-server --project-name my-api-server --template rust_axum

# Configure server port and log file
agenterra scaffold mcp server --schema-path api.yaml --output-dir my-server --port 8080 --log-file my-server
```

### Client Generation

```bash
# Basic client generation
agenterra scaffold mcp client --project-name my-client --output-dir generated-client

# Client with custom timeout
agenterra scaffold mcp client --project-name my-client --output-dir my-client --timeout 30

# Client with specific template
agenterra scaffold mcp client --project-name my-client --output-dir my-client --template rust_reqwest
```

### Advanced Examples

```bash
# Generate both server and client for the same API
agenterra scaffold mcp server --schema-path petstore.json --output-dir petstore-server --project-name petstore-server
agenterra scaffold mcp client --project-name petstore-client --output-dir petstore-client

# Use custom template directory
agenterra scaffold mcp server --schema-path api.yaml --output-dir custom-server --template custom --template-dir ./my-templates/server
agenterra scaffold mcp client --project-name custom-client --output-dir custom-client --template custom --template-dir ./my-templates/client
```

## Exit Codes

| Code | Description |
|------|-------------|
| 0    | Success |
| 1    | General error |
| 2    | Invalid command line arguments |
| 3    | File I/O error |
| 4    | Template processing error |
| 5    | OpenAPI spec validation error |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `AGENTERRA_TEMPLATE` | Default template to use |
| `AGENTERRA_TEMPLATE_DIR` | Default template directory |
| `AGENTERRA_LOG_LEVEL` | Log level (debug, info, warn, error) |

Note: Command-line arguments take precedence over environment variables.

## Migration from Previous Versions

If you're upgrading from a previous version of Agenterra, note the breaking CLI changes:

**Old syntax (deprecated):**
```bash
agenterra scaffold --template-kind rust_axum --schema-path api.yaml --output-dir server
```

**New syntax:**
```bash
# For server generation
agenterra scaffold mcp server --template rust_axum --schema-path api.yaml --output-dir server

# For client generation (new feature)
agenterra scaffold mcp client --project-name my-client --output-dir client
```

## See Also

- [Configuration Guide](CONFIGURATION.md)
- [Templates Documentation](TEMPLATES.md)
- [Contributing Guide](../CONTRIBUTING.md)