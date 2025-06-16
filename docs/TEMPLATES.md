# Agenterra Template System

This document describes the template system used by Agenterra for code generation from OpenAPI specifications.

## Table of Contents
- [Template Structure](#template-structure)
- [Manifest Format](#manifest-format)
- [Available Template Variables](#available-template-variables)
- [Example Templates](#example-templates)
- [Template Context](#template-context)
- [Conditional Logic](#conditional-logic)
- [Including Other Templates](#including-other-templates)
- [Built-in Filters](#built-in-filters)

## Template Structure

Templates are organized hierarchically by protocol and role:

```
templates/
├── mcp/                    # Model Context Protocol templates
│   ├── server/             # MCP server templates
│   │   └── rust_axum/      # Rust Axum server template
│   │       ├── manifest.yaml
│   │       ├── Cargo.toml.tera
│   │       ├── src/
│   │       │   ├── main.rs.tera
│   │       │   ├── server.rs.tera
│   │       │   └── handlers/
│   │       │       └── mod.rs.tera
│   │       └── README.md.tera
│   └── client/             # MCP client templates
│       └── rust_reqwest/   # Rust reqwest client template
│           ├── manifest.yaml
│           ├── Cargo.toml.tera
│           ├── src/
│           │   ├── main.rs.tera
│           │   ├── client.rs.tera
│           │   └── repl.rs.tera
│           └── README.md.tera
└── future-protocols/       # Space for future protocol templates
    ├── a2a/               # Agent-to-Agent protocol (planned)
    └── custom/            # Custom protocol templates
```

Each template directory contains:
- `manifest.yaml` - Required template manifest
- `*.tera` - Template files using Tera templating engine
- Subdirectories for organized template structure

## Template Types

### Server Templates
Server templates generate MCP servers from OpenAPI specifications. They require:
- OpenAPI schema as input
- Base URL configuration
- Server-specific options (port, logging)

**Available Server Templates:**
- `rust_axum` - Rust MCP server using Axum web framework with rmcp protocol support

### Client Templates
Client templates generate MCP clients that can connect to MCP servers. They:
- Don't require OpenAPI schemas (discover tools at runtime)
- Focus on connection management and tool invocation
- Often include REPL or CLI interfaces

**Available Client Templates:**
- `rust_reqwest` - Rust MCP client with REPL interface using rmcp protocol

## Manifest Format

The `manifest.yaml` file defines the template's metadata and configuration:

```yaml
name: "rust_reqwest"     # Required: Template name
version: "0.1.0"         # Required: Template version  
description: >           # Optional: Template description
  Rust MCP client using reqwest HTTP client and rmcp protocol
language: "rust"         # Optional: Programming language
author: "Agenterra Team" # Optional: Author information

# Template options (client templates typically have fewer options)
options:
  # Client configuration
  client:
    timeout: 10           # Default connection timeout
    repl_enabled: true    # Include REPL interface
  
  # For server templates, you might have:
  # server:
  #   port: 8080          # Default server port
  #   log_file: app.log   # Default log file path
  #   all_operations: true # Include all operations

# Template files configuration
files:
  - source: "Cargo.toml.tera"
    destination: "Cargo.toml"
    
  - source: "src/main.rs.tera"
    destination: "src/main.rs"
    
  - source: "src/client.rs.tera"
    destination: "src/client.rs"
    context:              # Optional: Additional context for this file
      is_client: true
    
  - source: "README.md.tera"
    destination: "README.md"

# Hooks (optional)
hooks:
  post_generate: hooks/post-generate.sh  # Script to run after generation
```

## Available Template Variables

### Global Variables

| Variable           | Type     | Description                                      |
|--------------------|----------|--------------------------------------------------|
| `project_name`    | String   | Name of the generated project                    |
| `api_version`     | String   | API version from OpenAPI spec                    |
| `spec`            | Object   | The complete OpenAPI specification object        |
| `endpoints`       | Array    | List of endpoint contexts (see below)            |
| `current_time`    | DateTime | Current date and time                            |
| `template_opts`   | Object   | Template options from manifest                   |

### Endpoint Context

Each endpoint in the `endpoints` array has the following structure:

```rust
{
  endpoint: String,           // e.g., "get_pets"
  endpoint_cap: String,       // e.g., "GET_PETS"
  fn_name: String,           // e.g., "get_pets"
  parameters_type: String,   // e.g., "GetPetsParams"
  properties_type: String,   // e.g., "PetProperties"
  response_type: String,     // e.g., "PetResponse"
  envelope_properties: Value, // JSON schema of response properties
  properties: Vec<PropertyInfo>,
  properties_for_handler: Vec<String>,
  parameters: Vec<ParameterInfo>,
  summary: String,
  description: String,
  tags: Vec<String>,
  properties_schema: Map<String, Value>,
  response_schema: Value,
  spec_file_name: Option<String>,
  valid_fields: Vec<String>
}
```

### PropertyInfo

```rust
struct PropertyInfo {
    name: String,
    rust_type: String,
    title: Option<String>,
    description: Option<String>,
    example: Option<Value>
}
```

### ParameterInfo

```rust
struct ParameterInfo {
    name: String,
    rust_type: String,
    description: Option<String>,
    example: Option<Value>
}
```

## Example Templates

### MCP Client Template Example (`client.rs.tera`)

```rust
// Generated by Agenterra - {{ current_time }}
// MCP Client for {{ project_name | default(value="MCP Server") }}

use anyhow::{Result, Context};
use rmcp::{Client, ToolCall, ToolResult};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

pub struct McpClient {
    client: Option<Client>,
    server_url: String,
    timeout: Duration,
    tools: HashMap<String, Value>,
}

impl McpClient {
    pub async fn new(server_url: &str, timeout_secs: u64) -> Result<Self> {
        Ok(Self {
            client: None,
            server_url: server_url.to_string(),
            timeout: Duration::from_secs(timeout_secs),
            tools: HashMap::new(),
        })
    }

    pub async fn connect(&mut self) -> Result<()> {
        let client = Client::stdio()
            .context("Failed to create stdio MCP client")?;
        self.client = Some(client);
        self.discover_tools().await?;
        Ok(())
    }

    async fn discover_tools(&mut self) -> Result<()> {
        // Tool discovery implementation
        Ok(())
    }
}
```

### MCP Server Template Example (`handlers/mod.rs.tera`)

```rust
// Generated by Agenterra - {{ current_time }}
// MCP Handlers for {{ project_name | default(value="MCP Server") }}

use rmcp::McpService;
use serde_json::Value;

pub struct McpServer {
    // Server state
}

#[rmcp::async_trait]
impl McpService for McpServer {
    async fn list_tools(&self) -> rmcp::Result<Vec<rmcp::Tool>> {
        let tools = vec![
            {% for endpoint in endpoints %}
            rmcp::Tool {
                name: "{{ endpoint.fn_name }}".to_string(),
                description: Some("{{ endpoint.summary }}".to_string()),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        {% for param in endpoint.parameters %}
                        "{{ param.name }}": {
                            "type": "{{ param.rust_type | type_rs }}",
                            "description": "{{ param.description | default(value="") }}"
                        }{% if not loop.last %},{% endif %}
                        {% endfor %}
                    }
                }),
            },
            {% endfor %}
        ];
        Ok(tools)
    }

    async fn call_tool(&self, call: rmcp::ToolCall) -> rmcp::Result<Vec<rmcp::ToolResult>> {
        match call.name.as_str() {
            {% for endpoint in endpoints %}
            "{{ endpoint.fn_name }}" => {
                // Implementation for {{ endpoint.fn_name }}
                Ok(vec![rmcp::ToolResult {
                    content: vec![rmcp::TextContent {
                        text: "Result from {{ endpoint.fn_name }}".to_string(),
                    }.into()],
                    is_error: Some(false),
                }])
            }
            {% endfor %}
            _ => Err(rmcp::McpError::MethodNotFound(call.name)),
        }
    }
}
```

### REPL Template Example (`repl.rs.tera`)

```rust
// Generated by Agenterra - {{ current_time }}
// REPL interface for {{ project_name | default(value="MCP Client") }}

use anyhow::Result;
use rustyline::{Editor, Result as RustylineResult};
use crate::client::McpClient;

pub struct McpRepl {
    client: McpClient,
    editor: Editor<()>,
}

impl McpRepl {
    pub fn new(client: McpClient) -> Self {
        let editor = Editor::new();
        Self { client, editor }
    }

    pub async fn run(&mut self) -> Result<()> {
        println!("🚀 {{ project_name | default(value="MCP Client") }} REPL");
        println!("Type 'help' for available commands, 'quit' to exit");
        
        loop {
            match self.editor.readline("> ") {
                Ok(line) => {
                    self.editor.add_history_entry(&line);
                    
                    match line.trim() {
                        "quit" | "exit" => break,
                        "help" => self.show_help(),
                        "tools" => self.list_tools().await?,
                        cmd if cmd.starts_with("call ") => {
                            let tool_name = &cmd[5..];
                            self.call_tool(tool_name).await?;
                        }
                        _ => println!("Unknown command: {}", line.trim()),
                    }
                }
                Err(_) => break,
            }
        }
        
        Ok(())
    }

    fn show_help(&self) {
        println!("Available commands:");
        println!("  tools           - List available tools");
        println!("  call <tool>     - Call a specific tool");
        println!("  help            - Show this help");
        println!("  quit/exit       - Exit the REPL");
    }

    async fn list_tools(&mut self) -> Result<()> {
        let tools = self.client.get_tools();
        if tools.is_empty() {
            println!("No tools available");
        } else {
            println!("Available tools:");
            for (name, tool) in tools {
                println!("  {} - {}", name, tool.description.as_deref().unwrap_or("No description"));
            }
        }
        Ok(())
    }

    async fn call_tool(&mut self, tool_name: &str) -> Result<()> {
        println!("Calling tool: {}", tool_name);
        // Tool invocation implementation
        Ok(())
    }
}
```

## Template Context

Templates have access to a rich context that includes:

1. **Global Context**: Available in all templates
   - `project_name`: Name of the generated project
   - `api_version`: Version from the OpenAPI spec
   - `current_time`: Timestamp of generation
   - `template_opts`: Options from the manifest

2. **Server-Specific Context**: Server templates get:
   - `endpoints`: Array of API endpoints from OpenAPI spec  
   - `spec`: Complete OpenAPI specification
   - All global context variables

3. **Client-Specific Context**: Client templates get:
   - No endpoint or spec data (clients discover at runtime)
   - Focus on connection and tool invocation patterns
   - All global context variables

## Conditional Logic

You can use Tera's control structures for conditional generation:

```jinja
{% if endpoint.tags contains "admin" %}
// This is an admin-only endpoint
#[requires_role("admin")]
{% endif %}
```

## Including Other Templates

Use Tera's `include` to reuse template fragments:

```jinja
{% include "common/header.tera" %}

// Your template content here

{% include "common/footer.tera" %}
```

## Built-in Filters

Agenterra includes several useful Tera filters:

- `camel_case`: Convert string to camelCase
- `pascal_case`: Convert string to PascalCase
- `snake_case`: Convert string to snake_case
- `kebab_case`: Convert string to kebab-case
- `json_encode`: Convert value to JSON string
- `type_rs`: Convert OpenAPI type to Rust type

Example:
```jinja
{{ "user_name" | snake_case }}  // user_name
{{ "user_name" | camel_case }}  // userName
{{ "user_name" | pascal_case }} // UserName
{{ "user_name" | kebab_case }}  // user-name
{{ endpoint.parameters | json_encode | safe }}
{{ "string" | type_rs }}  // String
```

## Creating Custom Templates

### Custom Server Template

To create a custom server template:

1. Create directory structure:
   ```
   templates/mcp/server/my_custom_server/
   ├── manifest.yaml
   ├── Cargo.toml.tera
   └── src/
       ├── main.rs.tera
       └── lib.rs.tera
   ```

2. Use the template:
   ```bash
   agenterra scaffold mcp server --template my_custom_server --schema-path api.yaml
   ```

### Custom Client Template

To create a custom client template:

1. Create directory structure:
   ```
   templates/mcp/client/my_custom_client/
   ├── manifest.yaml
   ├── package.json.tera  # For Node.js client
   └── src/
       ├── index.ts.tera
       └── client.ts.tera
   ```

2. Use the template:
   ```bash
   agenterra scaffold mcp client --template my_custom_client --project-name my-client
   ```

## Best Practices

1. **Organize by protocol and role**: Follow the `templates/{protocol}/{role}/{template}` structure
2. **Keep templates focused**: Server templates handle OpenAPI, client templates handle MCP communication
3. **Use includes**: Break down large templates into smaller, reusable components
4. **Document templates**: Add comments explaining non-obvious parts
5. **Test thoroughly**: Generate code and verify it compiles and works
6. **Handle optional fields**: Always check if fields exist before accessing them
7. **Consider the target use case**: Servers need robustness, clients need usability

## Template Context Differences

### Server Templates
Server templates receive:
- `endpoints` - Array of API endpoints from OpenAPI spec
- `spec` - Complete OpenAPI specification
- `api_version` - API version from spec
- `project_name` - Generated project name

### Client Templates  
Client templates receive:
- `project_name` - Generated project name
- `template_opts` - Template options from manifest
- `current_time` - Generation timestamp
- No `endpoints` or `spec` (clients discover tools at runtime)

## Troubleshooting

- **Missing variables**: Check if you're using server-specific variables in client templates
- **Template errors**: Check Tera's error messages for syntax issues
- **Incorrect output**: Verify your template logic and variable usage
- **Template not found**: Ensure template is in correct `templates/mcp/{role}/` directory
- **Compilation errors**: Verify generated code syntax and imports
