#{issue-number} Short summary of changes (50 chars or less)

# NOTE: The below section is optional and can be left blank if your commit is small and self-explanatory
# ### Why is this change necessary?
# (Describe the problem, motivation, or context)
# ### How does it address the issue?
# (Explain what you did and why it solves the problem)
# ### Side effects, risks, or other impacts:
# - List any breaking changes, migrations, or downstream effects
# - Note if additional testing or documentation is required
# ### How was this tested?
# - (Unit tests, integration tests, manual QA, etc)
#
# (Recommended) [WIP, Fixes, Closes]: #{issue-number}
# (Optional) [Sibling Of, Relates To, Depends On]: #{issue-number}

# Format guide:
# - First line: Start with issue number (ex:#123) and short summary
# - Second line: Leave blank
# - Use imperative mood (ex: "Add feature" not "Added feature")
# - Do not end with a period
# - Limit line length to 72 characters
# - Reference issues and pull requests using `#{issue-number}` pattern
# - Use Mark down syntax for all sections but the first line
#
# Example:
# #101 Refactor OpenAPI parameter serialization for MCP endpoints

# # Why is this change necessary?
# Refactor needed to ensure OpenAPI output matches official spec and improves clarity for API consumers.

# # How does it address the issue?
# - Updated OpenAPI JSON generation to match spec for parameter objects
# - Added helper to map ParameterInfo to OpenAPI-compliant objects
# - Improved validation and error messaging for endpoint parameters

# # Side effects, risks, or other impacts:
# - No breaking changes, but downstream consumers will see improved parameter docs
# - Additional unit and integration tests added for parameter serialization

# # How was this tested?
# - Ran cargo test for all handlers and OpenAPI output
# - Manual QA of Swagger UI and OpenAPI JSON

# Fixes: #101
# Relates To: #99