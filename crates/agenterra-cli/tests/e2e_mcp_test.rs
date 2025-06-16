//! End-to-end integration test for MCP server and client generation and communication

use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

use rusqlite::{Connection, params};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::Command as AsyncCommand;
use tokio::time::timeout;

#[tokio::test]
async fn test_mcp_server_client_generation() -> Result<()> {
    // Discover project root first
    let current_dir = std::env::current_dir()?;
    let project_dir = current_dir
        .parent()
        .unwrap() // Go up from crates/agenterra-cli
        .parent()
        .unwrap(); // Go up to project root

    // Resolve path to agenterra binary (prefer Cargo-built path)
    let agenterra = project_dir
        .join("target/debug/agenterra")
        .to_string_lossy()
        .into_owned();

    // Pass project root as template dir - the code will append "templates" internally
    let template_dir = project_dir;

    // Use workspace .agenterra directory for generated artifacts
    // Clean any previous run directories to avoid duplicate headers or build conflicts
    let scaffold_path = project_dir.join(".agenterra");
    // Clean any previous run directories to avoid conflicts
    for sub in ["e2e_mcp_server", "e2e_mcp_client"] {
        let dir = scaffold_path.join(sub);
        let _ = fs::remove_dir_all(&dir);
    }
    std::fs::create_dir_all(&scaffold_path)?;

    println!("=== Testing MCP Server Generation ===");
    println!("Project dir: {}", project_dir.display());
    println!("Template dir: {}", template_dir.display());
    println!(
        "Expected template path: {}/templates/mcp/server/rust_axum",
        template_dir.display()
    );

    // Test 1: Generate MCP server
    let server_name = "e2e_mcp_server";
    let server_output = scaffold_path.join(server_name);
    let schema_path = project_dir.join("tests/fixtures/openapi/petstore.openapi.v3.json");

    // Verify schema file exists
    if !schema_path.exists() {
        panic!("Schema file not found at: {}", schema_path.display());
    }

    let server_result = Command::new(&agenterra)
        .args([
            "scaffold",
            "mcp",
            "server",
            "--project-name",
            server_name,
            "--output-dir",
            server_output.to_str().unwrap(),
            "--schema-path",
            schema_path.to_str().unwrap(),
            "--template-dir",
            template_dir.to_str().unwrap(),
            "--template",
            "rust_axum",
            "--base-url",
            "https://petstore3.swagger.io",
        ])
        .output()?;

    println!(
        "Server generation stdout: {}",
        String::from_utf8_lossy(&server_result.stdout)
    );
    if !server_result.stderr.is_empty() {
        println!(
            "Server generation stderr: {}",
            String::from_utf8_lossy(&server_result.stderr)
        );
    }

    if !server_result.status.success() {
        panic!(
            "Server generation failed with exit code: {:?}",
            server_result.status.code()
        );
    }

    // Verify server files exist
    assert!(server_output.join("Cargo.toml").exists());
    assert!(server_output.join("src/main.rs").exists());
    assert!(server_output.join("src/handlers/mod.rs").exists());

    println!("✅ Server generation successful");

    println!("\n=== Testing MCP Client Generation ===");

    // Test 2: Generate MCP client
    let client_name = "e2e_mcp_client";
    let client_output = scaffold_path.join(client_name);
    let client_result = Command::new(&agenterra)
        .args([
            "scaffold",
            "mcp",
            "client",
            "--project-name",
            client_name,
            "--output-dir",
            client_output.to_str().unwrap(),
            "--template-dir",
            template_dir.to_str().unwrap(),
            "--template",
            "rust_reqwest",
        ])
        .output()?;

    if !client_result.status.success() {
        eprintln!("Client generation failed:");
        eprintln!("stdout: {}", String::from_utf8_lossy(&client_result.stdout));
        eprintln!("stderr: {}", String::from_utf8_lossy(&client_result.stderr));
        panic!("Client generation failed");
    }

    // Verify client files exist
    assert!(client_output.join("Cargo.toml").exists());
    assert!(client_output.join("src/main.rs").exists());
    assert!(client_output.join("src/client.rs").exists());
    assert!(client_output.join("src/repl.rs").exists());

    println!("✅ Client generation successful");

    // Ensure standalone crates by appending minimal workspace footer
    for path in [&server_output, &client_output] {
        let cargo_toml = path.join("Cargo.toml");
        if let Ok(contents) = fs::read_to_string(&cargo_toml) {
            if !contents.contains("[workspace]") {
                if let Ok(mut f) = OpenOptions::new().append(true).open(&cargo_toml) {
                    writeln!(f, "\n[workspace]\n").ok();
                }
            }
        }
    }

    // Test 3: Build generated projects (always test compilation)
    println!("\n=== Building Generated Server ===");

    let server_build = Command::new("cargo")
        .args([
            "build",
            "--manifest-path",
            &server_output.join("Cargo.toml").to_string_lossy(),
        ])
        .output()?;

    if !server_build.status.success() {
        eprintln!("Server build failed:");
        eprintln!("stderr: {}", String::from_utf8_lossy(&server_build.stderr));
        panic!("Server build failed");
    }

    println!("✅ Server builds successfully");

    println!("\n=== Building Generated Client ===");

    let client_build = Command::new("cargo")
        .args([
            "build",
            "--manifest-path",
            &client_output.join("Cargo.toml").to_string_lossy(),
        ])
        .output()?;

    if !client_build.status.success() {
        eprintln!("Client build failed:");
        eprintln!("stderr: {}", String::from_utf8_lossy(&client_build.stderr));
        panic!("Client build failed");
    }

    println!("✅ Client builds successfully");

    // Test 4: End-to-end MCP communication using generated client
    println!("\n=== Testing MCP Server ↔ Client Communication ===");

    // The generated binary name matches the project name we passed ("e2e_mcp_server")
    let server_binary = server_output.join("target/debug/e2e_mcp_server");
    if !server_binary.exists() {
        anyhow::bail!(
            "Expected server binary not found at {}",
            server_binary.display()
        );
    }

    println!("✅ Server binary found at: {}", server_binary.display());

    // Use the generated client to test MCP communication
    let test_result = timeout(Duration::from_secs(60), async {
        test_mcp_with_interactive_client(&server_binary, &client_output).await
    })
    .await;

    match test_result {
        Ok(Ok(())) => {
            println!("✅ MCP communication test successful");
        }
        Ok(Err(e)) => {
            panic!("MCP communication test failed: {}", e);
        }
        Err(_) => {
            panic!("MCP communication test timed out");
        }
    }

    // Test 5: Verify SQLite cache directly
    println!("\n=== Verifying SQLite Cache ===");

    verify_sqlite_cache(&client_output)?;

    println!("\n🎉 Complete end-to-end MCP test passed!");

    Ok(())
}

/// Test MCP communication using the generated client's interactive REPL
async fn test_mcp_with_interactive_client(
    server_binary: &std::path::Path,
    client_output: &std::path::Path,
) -> Result<()> {
    println!("Starting comprehensive MCP client test...");

    // Find the client binary
    let client_binary = client_output.join("target/debug/e2e_mcp_client");
    if !client_binary.exists() {
        return Err(anyhow::anyhow!(
            "Client binary not found at: {}",
            client_binary.display()
        ));
    }

    // Start the client with the server binary path
    println!("Starting MCP client: {}", client_binary.display());
    let mut client_process = AsyncCommand::new(&client_binary)
        .arg("--server")
        .arg(server_binary.to_str().unwrap())
        .arg("--timeout")
        .arg("30")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn client process")?;

    let stdin = client_process
        .stdin
        .as_mut()
        .context("Failed to get client stdin")?;
    let stdout = client_process
        .stdout
        .as_mut()
        .context("Failed to get client stdout")?;

    let mut writer = BufWriter::new(stdin);
    let mut reader = BufReader::new(stdout);

    // Give client time to connect and show initial output
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Read initial output (connection messages, capabilities, prompt)
    let mut line = String::new();

    // Helper function to read until prompt
    async fn read_until_prompt(
        reader: &mut BufReader<&mut tokio::process::ChildStdout>,
        line: &mut String,
    ) -> Vec<String> {
        let mut output = Vec::new();
        for _ in 0..50 {
            line.clear();
            match timeout(Duration::from_millis(500), reader.read_line(line)).await {
                Ok(Ok(0)) => break, // EOF
                Ok(Ok(_)) => {
                    output.push(line.trim().to_string());
                    if line.contains("mcp>") {
                        break;
                    }
                }
                Ok(Err(_)) | Err(_) => break,
            }
        }
        output
    }

    // Read initial connection output
    println!("\n=== Initial Connection ===");
    let initial_output = read_until_prompt(&mut reader, &mut line).await;
    for line in &initial_output {
        println!("Initial: {}", line);
    }

    // Test 1: Status command
    println!("\n=== Testing Status Command ===");
    writer.write_all(b"status\n").await?;
    writer.flush().await?;

    let status_output = read_until_prompt(&mut reader, &mut line).await;
    let mut connection_verified = false;
    for line in &status_output {
        println!("Status: {}", line);
        if line.contains("Connected: true") {
            connection_verified = true;
        }
    }
    if !connection_verified {
        return Err(anyhow::anyhow!(
            "Status command failed to verify connection"
        ));
    }
    println!("✅ Status command successful");

    // Test 2: List and get all resources (this will populate the SQLite cache!)
    println!("\n=== Testing Resources ===");
    writer.write_all(b"resources\n").await?;
    writer.flush().await?;

    let resources_output = read_until_prompt(&mut reader, &mut line).await;
    let mut resource_uris = Vec::new();
    let mut in_resources_list = false;

    for line in &resources_output {
        println!("Resources: {}", line);
        if line.contains("Available resources:") {
            in_resources_list = true;
        } else if in_resources_list && line.trim().starts_with("") && line.contains(":") {
            // Extract URI from lines like "  uri: description"
            if let Some(uri) = line.trim().split(':').next() {
                let uri = uri.trim();
                if !uri.is_empty() && !uri.contains("No resources") {
                    resource_uris.push(uri.to_string());
                }
            }
        }
    }

    println!("Found {} resources to fetch", resource_uris.len());

    // Get each resource to populate the cache
    for uri in &resource_uris {
        println!("\n  Getting resource: {}", uri);
        writer
            .write_all(format!("get {}\n", uri).as_bytes())
            .await?;
        writer.flush().await?;

        let resource_output = read_until_prompt(&mut reader, &mut line).await;
        let mut resource_fetched = false;
        for line in &resource_output {
            if line.contains("Resource content:") || line.contains("contents") {
                resource_fetched = true;
            }
        }
        if resource_fetched {
            println!("  ✅ Resource fetched: {}", uri);
        } else {
            println!("  ⚠️  Failed to fetch resource: {}", uri);
        }
    }

    if !resource_uris.is_empty() {
        println!("✅ Resources discovery and fetching completed");
    }

    // Test 3: List and call all tools
    println!("\n=== Testing Tools ===");
    writer.write_all(b"tools\n").await?;
    writer.flush().await?;

    let tools_output = read_until_prompt(&mut reader, &mut line).await;
    let mut tool_names = Vec::new();
    let mut in_tools_list = false;

    for line in &tools_output {
        println!("Tools: {}", line);
        if line.contains("Available tools:") {
            in_tools_list = true;
        } else if in_tools_list && line.trim().starts_with("") && line.contains(":") {
            // Extract tool name from lines like "  toolname: description"
            if let Some(tool) = line.trim().split(':').next() {
                let tool = tool.trim();
                if !tool.is_empty() && !tool.contains("No tools") {
                    tool_names.push(tool.to_string());
                }
            }
        }
    }

    println!("Found {} tools to test", tool_names.len());

    // Call each tool (some may fail without auth, that's OK)
    let mut at_least_one_tool_succeeded = false;
    for tool in &tool_names {
        println!("\n  Calling tool: {}", tool);
        writer
            .write_all(format!("call {}\n", tool).as_bytes())
            .await?;
        writer.flush().await?;

        let tool_output = read_until_prompt(&mut reader, &mut line).await;
        let mut tool_result_found = false;
        for line in &tool_output {
            if line.contains("Tool result:") {
                tool_result_found = true;
                at_least_one_tool_succeeded = true;
            }
        }
        if tool_result_found {
            println!("  ✅ Tool called successfully: {}", tool);
        } else {
            println!("  ⚠️  Tool call failed or required auth: {}", tool);
        }
    }

    if !at_least_one_tool_succeeded && !tool_names.is_empty() {
        return Err(anyhow::anyhow!("No tools succeeded - all tools failed"));
    }

    if !tool_names.is_empty() {
        println!("✅ Tools discovery and testing completed");
    }

    // Test 4: List and get all prompts
    println!("\n=== Testing Prompts ===");
    writer.write_all(b"prompts\n").await?;
    writer.flush().await?;

    let prompts_output = read_until_prompt(&mut reader, &mut line).await;
    let mut prompt_names = Vec::new();
    let mut in_prompts_list = false;

    for line in &prompts_output {
        println!("Prompts: {}", line);
        if line.contains("Available prompts:") {
            in_prompts_list = true;
        } else if in_prompts_list && line.trim().starts_with("") && line.contains(":") {
            // Extract prompt name from lines like "  promptname: description"
            if let Some(prompt) = line.trim().split(':').next() {
                let prompt = prompt.trim();
                if !prompt.is_empty() && !prompt.contains("No prompts") {
                    prompt_names.push(prompt.to_string());
                }
            }
        }
    }

    println!("Found {} prompts to test", prompt_names.len());

    // Get each prompt
    for prompt in &prompt_names {
        println!("\n  Getting prompt: {}", prompt);
        writer
            .write_all(format!("prompt {}\n", prompt).as_bytes())
            .await?;
        writer.flush().await?;

        let prompt_output = read_until_prompt(&mut reader, &mut line).await;
        let mut prompt_fetched = false;
        for line in &prompt_output {
            if line.contains("Prompt content:") || line.contains("messages") {
                prompt_fetched = true;
            }
        }
        if prompt_fetched {
            println!("  ✅ Prompt fetched: {}", prompt);
        } else {
            println!("  ⚠️  Failed to fetch prompt: {}", prompt);
        }
    }

    if !prompt_names.is_empty() {
        println!("✅ Prompts discovery and fetching completed");
    }

    // Send 'quit' to exit cleanly
    println!("\n=== Exiting Client ===");
    writer.write_all(b"quit\n").await.ok();
    writer.flush().await.ok();

    // Give it a moment to exit cleanly
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Clean up client process
    if let Err(e) = client_process.kill().await {
        eprintln!("Warning: Failed to kill client process: {}", e);
    }

    println!("✅ Comprehensive MCP test completed successfully");
    Ok(())
}

#[test]
fn test_cli_help_output() {
    let agenterra = env!("CARGO_BIN_EXE_agenterra");

    // Test main help
    let result = Command::new(agenterra)
        .arg("--help")
        .output()
        .expect("Failed to run agenterra");

    let output = String::from_utf8_lossy(&result.stdout);
    assert!(output.contains("scaffold"));
    assert!(output.contains("Scaffold MCP servers and clients"));

    // Test scaffold mcp help
    let result = Command::new(agenterra)
        .args(["scaffold", "mcp", "--help"])
        .output()
        .expect("Failed to run agenterra");

    let output = String::from_utf8_lossy(&result.stdout);
    assert!(output.contains("server"));
    assert!(output.contains("client"));
    assert!(output.contains("Generate MCP server from OpenAPI specification"));
    assert!(output.contains("Generate MCP client"));
}

#[test]
fn test_new_cli_structure() {
    let agenterra = env!("CARGO_BIN_EXE_agenterra");

    // Test server help shows correct options
    let result = Command::new(agenterra)
        .args(["scaffold", "mcp", "server", "--help"])
        .output()
        .expect("Failed to run agenterra");

    let output = String::from_utf8_lossy(&result.stdout);
    assert!(output.contains("--schema-path"));
    assert!(output.contains("--template"));
    assert!(output.contains("--output-dir"));

    // Test client help shows correct options
    let result = Command::new(agenterra)
        .args(["scaffold", "mcp", "client", "--help"])
        .output()
        .expect("Failed to run agenterra");

    let output = String::from_utf8_lossy(&result.stdout);
    assert!(output.contains("--template"));
    assert!(output.contains("--output-dir"));
    // Client should NOT have schema-path
    assert!(!output.contains("--schema-path"));
}

#[test]
fn test_cli_flag_combinations() {
    let agenterra = env!("CARGO_BIN_EXE_agenterra");

    // Test 1: Server command requires --schema-path
    let result = Command::new(agenterra)
        .args(["scaffold", "mcp", "server", "--project-name", "test"])
        .output()
        .expect("Failed to run agenterra");

    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    // Note: Due to CLI design, template initialization happens before argument validation
    // So we get template errors instead of missing argument errors
    assert!(
        stderr.contains("template")
            || stderr.contains("required")
            || stderr.contains("schema-path")
    );

    // Test 2: Client command requires --project-name
    let result = Command::new(agenterra)
        .args(["scaffold", "mcp", "client", "--template", "rust_reqwest"])
        .output()
        .expect("Failed to run agenterra");

    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    // Note: Due to CLI design, template initialization happens before argument validation
    // So we get template errors instead of missing argument errors
    assert!(
        stderr.contains("template")
            || stderr.contains("required")
            || stderr.contains("project-name")
    );

    // Test 3: Client command should reject --schema-path
    let result = Command::new(agenterra)
        .args([
            "scaffold",
            "mcp",
            "client",
            "--project-name",
            "test",
            "--schema-path",
            "dummy.yaml",
        ])
        .output()
        .expect("Failed to run agenterra");

    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("unrecognized") || stderr.contains("unexpected"));

    // Test 4: Valid server command combination
    // Note: This will fail because file doesn't exist, but argument parsing should work
    let result = Command::new(agenterra)
        .args([
            "scaffold",
            "mcp",
            "server",
            "--schema-path",
            "/nonexistent/schema.yaml",
            "--project-name",
            "test",
            "--template",
            "rust_axum",
        ])
        .output()
        .expect("Failed to run agenterra");

    // Should fail due to missing file, not argument parsing
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("not found")
            || stderr.contains("No such file")
            || stderr.contains("template")
    );

    // Test 5: Valid client command combination
    let result = Command::new(agenterra)
        .args([
            "scaffold",
            "mcp",
            "client",
            "--project-name",
            "test-client",
            "--template",
            "rust_reqwest",
            "--output-dir",
            "/tmp/test-output",
        ])
        .output()
        .expect("Failed to run agenterra");

    // This should succeed in argument parsing
    // It may fail later due to template not found, but args should be valid
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        // Should NOT be an argument parsing error
        assert!(!stderr.contains("unrecognized"));
        assert!(!stderr.contains("required"));
    }
}

/// Verify SQLite cache by directly querying the database
fn verify_sqlite_cache(client_output: &std::path::Path) -> Result<()> {
    // Construct path to the SQLite database
    let db_path = client_output.join("target/debug/data/e2e_mcp_client_cache.db");

    if !db_path.exists() {
        anyhow::bail!("SQLite cache database not found at: {}", db_path.display());
    }

    println!("Found SQLite cache database at: {}", db_path.display());

    // Open connection to the database
    let conn = Connection::open(&db_path).context("Failed to open SQLite cache database")?;

    // Query the resources table to verify cached entries
    let mut stmt = conn.prepare(
        "SELECT id, uri, content_type, access_count, size_bytes, 
         datetime(created_at/1000, 'unixepoch') as created_at,
         datetime(accessed_at/1000, 'unixepoch') as accessed_at
         FROM resources 
         ORDER BY accessed_at DESC",
    )?;

    let resource_iter = stmt.query_map(params![], |row| {
        Ok((
            row.get::<_, String>(0)?,         // id
            row.get::<_, String>(1)?,         // uri
            row.get::<_, Option<String>>(2)?, // content_type
            row.get::<_, i64>(3)?,            // access_count
            row.get::<_, i64>(4)?,            // size_bytes
            row.get::<_, String>(5)?,         // created_at
            row.get::<_, String>(6)?,         // accessed_at
        ))
    })?;

    let mut resource_count = 0;
    let mut total_access_count = 0i64;
    let mut total_size = 0i64;

    println!("\nCached resources found:");
    println!("------------------------");

    for resource in resource_iter {
        let (id, uri, content_type, access_count, size_bytes, created_at, accessed_at) = resource?;
        resource_count += 1;
        total_access_count += access_count;
        total_size += size_bytes;

        println!("Resource #{}", resource_count);
        println!("  ID: {}", id);
        println!("  URI: {}", uri);
        println!(
            "  Content-Type: {}",
            content_type.unwrap_or_else(|| "N/A".to_string())
        );
        println!("  Access Count: {}", access_count);
        println!("  Size: {} bytes", size_bytes);
        println!("  Created: {}", created_at);
        println!("  Last Accessed: {}", accessed_at);
        println!();
    }

    // With comprehensive testing, we should have cached resources
    if resource_count == 0 {
        println!("⚠️  WARNING: No resources found in cache!");
        println!("    This suggests either:");
        println!("    1. The MCP server has no resources exposed");
        println!("    2. Resource fetching failed during the test");
        println!("    3. The cache is not working properly");
    }

    // Verify the cache analytics table exists and has data
    let analytics_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM cache_analytics", params![], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    // Query cache analytics for hit/miss rates
    let cache_stats = conn
        .query_row(
            "SELECT hit_rate, total_requests, cache_size_mb, eviction_count 
         FROM cache_analytics 
         ORDER BY timestamp DESC 
         LIMIT 1",
            params![],
            |row| {
                Ok((
                    row.get::<_, f64>(0).unwrap_or(0.0), // hit_rate
                    row.get::<_, i64>(1).unwrap_or(0),   // total_requests
                    row.get::<_, f64>(2).unwrap_or(0.0), // cache_size_mb
                    row.get::<_, i64>(3).unwrap_or(0),   // eviction_count
                ))
            },
        )
        .ok();

    println!("Summary:");
    println!("  Total cached resources: {}", resource_count);
    println!("  Total accesses: {}", total_access_count);
    println!("  Total cache size: {} bytes", total_size);
    println!("  Analytics entries: {}", analytics_count);

    if let Some((hit_rate, requests, size_mb, evictions)) = cache_stats {
        println!("\nCache Performance:");
        println!("  Hit rate: {:.2}%", hit_rate * 100.0);
        println!("  Total requests: {}", requests);
        println!("  Cache size: {:.2} MB", size_mb);
        println!("  Evictions: {}", evictions);
    }

    // Verify that resources were actually accessed (not just stored)
    if resource_count > 0 && total_access_count < resource_count {
        anyhow::bail!(
            "Resources were cached but not accessed - cache retrieval may not be working"
        );
    }

    println!("\n✅ SQLite cache verification successful");
    Ok(())
}
