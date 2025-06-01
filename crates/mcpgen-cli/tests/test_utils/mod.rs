//! Test utilities for MCPGen integration tests

// Internal imports (std, crate)
use std::fs;
use std::path::{Path, PathBuf};

// External imports (alphabetized)
use anyhow::Context;
use tempfile::TempDir;

/// Creates a temporary directory for test outputs
pub fn create_temp_dir() -> anyhow::Result<(TempDir, PathBuf)> {
    let temp_dir = tempfile::tempdir()?;
    let temp_path = temp_dir.path().to_path_buf();
    Ok((temp_dir, temp_path))
}

/// Creates a test OpenAPI spec file in the given directory
pub fn create_test_openapi_spec(dir: &Path) -> anyhow::Result<PathBuf> {
    let spec_path = dir.join("openapi.yaml");
    let spec_content = r#"
openapi: 3.0.0
info:
  title: Test API
  version: 1.0.0
  description: Test API for MCPGen integration tests
  contact:
    name: API Support
    url: https://example.com/support
    email: support@example.com
  license:
    name: MIT
    url: https://opensource.org/licenses/MIT

servers:
  - url: http://localhost:8080/api/v1
    description: Development server

paths:
  /pets:
    get:
      operationId: listPets
      summary: List all pets
      description: Returns all pets from the system that the user has access to
      parameters:
        - $ref: '#/components/parameters/limitParam'
      responses:
        '200':
          description: A list of pets
          content:
            application/json:
              schema:
                type: array
                items:
                  $ref: '#/components/schemas/Pet'
        default:
          description: Unexpected error
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Error'
    
    post:
      operationId: createPet
      summary: Create a pet
      description: Creates a new pet in the store
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/NewPet'
      responses:
        '201':
          description: Pet created successfully
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Pet'
        default:
          $ref: '#/components/responses/Error'

  /pets/{petId}:
    get:
      operationId: getPetById
      summary: Get pet by ID
      description: Returns a single pet by ID
      parameters:
        - $ref: '#/components/parameters/petIdParam'
      responses:
        '200':
          description: A pet
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Pet'
        '404':
          description: Pet not found
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Error'
        default:
          $ref: '#/components/responses/Error'

components:
  schemas:
    Pet:
      type: object
      required:
        - id
        - name
      properties:
        id:
          type: integer
          format: int64
          example: 1
        name:
          type: string
          example: "doggie"
        tag:
          type: string
          example: "dog"
    
    NewPet:
      type: object
      required:
        - name
      properties:
        name:
          type: string
          example: "doggie"
        tag:
          type: string
          example: "dog"
    
    Error:
      type: object
      required:
        - code
        - message
      properties:
        code:
          type: integer
          format: int32
        message:
          type: string
  
  parameters:
    limitParam:
      name: limit
      in: query
      description: Maximum number of items to return
      required: false
      schema:
        type: integer
        format: int32
        minimum: 1
        default: 10
    
    petIdParam:
      name: petId
      in: path
      description: ID of pet to return
      required: true
      schema:
        type: integer
        format: int64
  
  responses:
    Error:
      description: Unexpected error
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/Error'"#;

    fs::write(&spec_path, spec_content)?;
    Ok(spec_path)
}

/// Asserts that a directory contains all the expected files
pub fn assert_dir_contains_files(dir: &Path, expected_files: &[&str]) -> anyhow::Result<()> {
    let mut missing_files = Vec::new();

    for file in expected_files {
        let path = dir.join(file);
        if !path.exists() {
            missing_files.push(path.display().to_string());
        }
    }

    if !missing_files.is_empty() {
        return Err(anyhow::anyhow!(
            "Missing expected files in {}:\n  {}",
            dir.display(),
            missing_files.join("\n  ")
        ));
    }

    Ok(())
}

/// Asserts that a file contains specific content
///
/// # Arguments
/// * `path` - Path to the file to check
/// * `contents` - Slice of string slices that should all be present in the file
///
/// # Returns
/// * `Ok(())` if all contents are found
/// * `Err` with a descriptive message if the file is missing or any content is not found
pub fn assert_file_contains<P: AsRef<Path>>(path: P, contents: &[&str]) -> anyhow::Result<()> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(anyhow::anyhow!("File not found: {}", path.display()));
    }

    let file_content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let mut missing_contents = Vec::new();
    for expected in contents {
        if !file_content.contains(*expected) {
            missing_contents.push(*expected);
        }
    }

    if !missing_contents.is_empty() {
        return Err(anyhow::anyhow!(
            "File {} is missing expected content:\n  {}",
            path.display(),
            missing_contents.join("\n  ")
        ));
    }

    Ok(())
}

/// Creates a basic template directory structure for testing
pub fn create_test_template_dir(dir: &Path) -> anyhow::Result<()> {
    // Create template directory
    let template_dir = dir.join("custom-rust-axum");
    fs::create_dir_all(&template_dir)?;

    // Create template manifest
    fs::write(template_dir.join("manifest.yaml"), TEMPLATE_MANIFEST)?;

    // Create template files
    fs::create_dir_all(template_dir.join("handlers"))?;
    fs::create_dir_all(template_dir.join("config"))?;
    fs::create_dir_all(template_dir.join("models"))?;
    fs::create_dir_all(template_dir.join("tests/common"))?;

    // Write template files with proper .tera extension and content
    fs::write(template_dir.join("main.rs.tera"), MAIN_RS_TEMPLATE)?;
    fs::write(
        template_dir.join("handlers/mod.rs.tera"),
        HANDLERS_MOD_TEMPLATE,
    )?;
    fs::write(
        template_dir.join("handlers/pet.rs.tera"),
        HANDLER_PET_TEMPLATE,
    )?;
    fs::write(template_dir.join("Cargo.toml.tera"), CARGO_TOML_TEMPLATE)?;

    // Create empty template files for other required files
    fs::write(template_dir.join("config/mod.rs.tera"), "// Config module")?;
    fs::write(template_dir.join("models/mod.rs.tera"), "// Models module")?;
    fs::write(
        template_dir.join("tests/common/mod.rs.tera"),
        "// Test utilities",
    )?;

    Ok(())
}

// Template content constants
const TEMPLATE_MANIFEST: &str = r#"
name: test-template
description: Test template for integration tests
author: MCPGen Tests
version: 0.1.0
files:
  - source: main.rs.tera
    destination: src/main.rs
  - source: handlers/mod.rs.tera
    destination: src/handlers/mod.rs
  - source: handlers/pet.rs.tera
    destination: src/handlers/pet.rs
  - source: Cargo.toml.tera
    destination: Cargo.toml
  - source: config/mod.rs.tera
    destination: src/config/mod.rs
  - source: models/mod.rs.tera
    destination: src/models/mod.rs
  - source: tests/common/mod.rs.tera
    destination: tests/common/mod.rs
"#;

const MAIN_RS_TEMPLATE: &str = r#"
use axum::{
    routing::get,
    Router,
};

mod handlers;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/pets", get(handlers::pet::list_pets));

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Server listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
"#;

const HANDLERS_MOD_TEMPLATE: &str = r#"
pub mod pet;

// Auto-generated handlers module"#;

const HANDLER_PET_TEMPLATE: &str = r#"
use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
};

pub async fn list_pets() -> Json<Vec<serde_json::Value>> {
    Json(vec![])
}
"#;

const CARGO_TOML_TEMPLATE: &str = r#"
[package]
name = "{{project_name}}"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.7"
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
"#;
