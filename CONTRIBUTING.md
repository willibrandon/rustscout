# Contributing to RustScout

First off, thank you for considering contributing to RustScout! It's people like you that make RustScout such a great tool.

## Code of Conduct

This project and everyone participating in it is governed by our Code of Conduct. By participating, you are expected to uphold this code.

## How Can I Contribute?

### Reporting Bugs

Before creating bug reports, please check the issue list as you might find out that you don't need to create one. When you are creating a bug report, please include as many details as possible:

* Use a clear and descriptive title
* Describe the exact steps which reproduce the problem
* Provide specific examples to demonstrate the steps
* Describe the behavior you observed after following the steps
* Explain which behavior you expected to see instead and why
* Include any relevant logs or error output

### Suggesting Enhancements

Enhancement suggestions are tracked as GitHub issues. When creating an enhancement suggestion, please include:

* Use a clear and descriptive title
* Provide a step-by-step description of the suggested enhancement
* Provide specific examples to demonstrate the steps
* Describe the current behavior and explain which behavior you expected to see instead
* Explain why this enhancement would be useful

### Pull Requests

* Fill in the required template
* Do not include issue numbers in the PR title
* Include screenshots and animated GIFs in your pull request whenever possible
* Follow the Rust styleguide
* Include thoughtfully-worded, well-structured tests
* Document new code
* End all files with a newline

## Development Process

1. Fork the repo
2. Create a new branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Run the tests (`cargo test`)
5. Run the benchmarks (`cargo bench`)
6. Commit your changes (`git commit -m 'Add some amazing feature'`)
7. Push to the branch (`git push origin feature/amazing-feature`)
8. Open a Pull Request

### Setting Up Development Environment

```bash
# Clone your fork
git clone https://github.com/your-username/rustscout.git
cd rustscout

# Add upstream remote
git remote add upstream https://github.com/willibrandon/rustscout.git

# Install development dependencies
cargo build

# Run tests
cargo test

# Run benchmarks
cargo bench
```

### Project Structure

```
rustscout/
├── rustscout/        # Core search functionality
│   ├── src/
│   │   ├── lib.rs    # Library entry point
│   │   ├── search.rs # Search implementation
│   │   ├── config.rs # Configuration handling
│   │   └── ...
│   └── benches/      # Performance benchmarks
├── rustscout-cli/    # Command-line interface
│   └── src/
│       └── main.rs   # CLI implementation
└── docs/            # Documentation
    └── blog/        # Blog posts
```

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run tests with output
cargo test -- --nocapture

# Run benchmarks
cargo bench
```

### Coding Style

We follow the standard Rust style guide. Please ensure your code:

* Uses 4 spaces for indentation
* Follows naming conventions
* Includes documentation comments
* Has no clippy warnings

You can check your style with:
```bash
cargo clippy
cargo fmt -- --check
```

### Writing Documentation

* Use clear and consistent terminology
* Include examples where appropriate
* Keep explanations concise but complete
* Update relevant README sections
* Add inline documentation for public APIs

### Performance Considerations

When contributing performance-related changes:

* Include before/after benchmarks
* Consider both small and large file cases
* Test with various pattern types
* Verify thread scaling behavior
* Document any tradeoffs made

## Community

* Join our Discord server for discussions
* Follow us on Twitter for updates
* Star the project if you find it useful!

## Questions?

Feel free to open an issue or reach out to the maintainers directly.

Thank you for contributing to RustScout! 🚀 