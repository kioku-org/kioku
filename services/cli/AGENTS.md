# Agent Guidelines

This document contains guidelines and best practices for AI agents working with this codebase.

## Error Management

- Use `anyhow::Result` for error handling in services and repositories.
- Create domain errors using `thiserror`.
- Never implement `From` for converting domain errors, manually convert them.

## Writing Tests

- All tests should be written in three discrete steps:

  ```rust,ignore
  use pretty_assertions::assert_eq; // Always use pretty assertions

  fn test_foo() {
      let setup = ...; // Instantiate a fixture or setup for the test
      let actual = ...; // Execute the fixture to create an output
      let expected = ...; // Define a hand written expected result
      assert_eq!(actual, expected); // Assert that the actual result matches the expected result
  }
  ```

- Use `pretty_assertions` for better error messages.
- Use `assert_eq!` for equality checks.
- Use `assert!(...)` for boolean checks.
- Use unwraps in test functions and `anyhow::Result` in fixtures.
- Keep the boilerplate to a minimum.

## Verification

Always verify changes by running tests and linting the codebase.

1. Run tests to ensure they pass:

   ```
   cargo test
   ```

2. Check formatting:

   ```
   cargo fmt --check
   ```

3. **Build Guidelines**:
   - **NEVER** run `cargo build --release` unless absolutely necessary
   - For verification, use `cargo check` (fastest) or `cargo test`

## Git Operations

- Safely assume git is pre-installed
- Safely assume github cli (gh) is pre-installed
- Always use `Co-Authored-By: Kioku <noreply@kioku.chat>` for git commits and Github comments
