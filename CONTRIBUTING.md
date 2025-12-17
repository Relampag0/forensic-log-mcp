# Contributing to Forensic Log MCP Server

Thank you for your interest in contributing! This document provides guidelines for contributing to the project.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/YOUR_USERNAME/forensic-log-mcp.git`
3. Create a branch: `git checkout -b feature/your-feature-name`

## Development Setup

### Prerequisites

- Rust 1.75+ (nightly recommended for edition 2024)
- Python 3.x (for benchmark scripts)
- Standard Unix tools: grep, awk, ripgrep, jq (for running benchmarks)

### Building

```bash
cd mcp
cargo build --release
```

### Running Tests

```bash
cd mcp
cargo test
```

## Code Style

- Follow standard Rust conventions
- Use `cargo fmt` before committing
- Run `cargo clippy` and address warnings
- Document public APIs with doc comments

## Performance Guidelines

This project prioritizes performance. When contributing:

1. **Benchmark your changes**: Run `benchmark/run_benchmark.sh` before and after
2. **Prefer zero-copy**: Work with `&[u8]` slices when possible
3. **Use SIMD paths**: For hot loops, consider `memchr` or similar
4. **Avoid allocations**: Pre-allocate buffers, reuse where possible

## Pull Request Process

1. Ensure your code builds without warnings
2. Update documentation if needed
3. Add tests for new functionality
4. Include benchmark results for performance-related changes
5. Write clear commit messages

## Commit Message Format

```
type: short description

Longer description if needed.

- Bullet points for details
- Reference issues: Fixes #123
```

Types: `feat`, `fix`, `perf`, `docs`, `test`, `refactor`, `chore`

## Adding New Log Formats

To add support for a new log format:

1. Create a parser in `mcp/src/parsers/`
2. Add format detection in `mcp/src/parsers/mod.rs`
3. Consider a SIMD fast path for common operations
4. Add benchmarks to `benchmark/run_benchmark.sh`
5. Document the format in the README

## Adding New Tools

MCP tools are defined in `mcp/src/tools/mod.rs`. To add a new tool:

1. Define the parameter struct with `schemars::JsonSchema`
2. Implement the tool logic
3. Register in the tool router
4. Document in `mcp/README.md`

## Reporting Issues

When reporting bugs:

- Include OS and Rust version
- Provide sample log files if possible
- Include the full error message
- Describe expected vs actual behavior

## Code of Conduct

- Be respectful and inclusive
- Focus on constructive feedback
- Help newcomers get started

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
