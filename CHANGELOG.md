# Changelog

All notable changes to rustscout will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]



## [1.1.0] - 2025-01-15

### Added
- Enhanced validation for empty patterns, invalid regex, and capture groups
- Adaptive file processing strategies based on file size
- Improved backup and undo system with JSON-formatted logs
- Preview functionality with dry-run option
- Detailed error messages for validation failures
- Blog post documenting replace module improvements

### Changed
- Update replace module to use "undo" subdirectory for better organization
- Improve error handling with descriptive messages
- Update documentation with validation and safety features
- Optimize file processing thresholds (32KB for small files, 10MB for large files)

### Fixed
- Fix empty pattern validation in replace module
- Fix invalid regex pattern handling
- Fix capture group reference validation
- Fix overlapping replacements detection
- Fix backup directory creation and path handling
- Fix undo information storage and retrieval

## [1.0.0] - 2025-01-15

### Added
- Word boundary search with Unicode support and smart hyphen handling
- Incremental search with smart caching
- Context lines feature with optimized implementation
- Search and replace functionality with backup and undo operations
- Support for multiple patterns in search
- Memory mapping support for large files
- Pattern caching for improved performance
- Memory usage tracking and metrics
- Repeated pattern search benchmarks
- Comprehensive Developer Guide with .NET comparisons
- Blog post for v1.0.0 release

### Changed
- Split search module into smaller components
- Update README with performance enhancements and new features
- Improve file handling and path operations
- Optimize pattern matching for large files
- Update CLI examples to consistently use rustscout-cli

### Fixed
- Resolve -i flag conflict
- Fix pattern caching test race conditions
- Fix formatting and clippy warnings
- Fix Windows-specific code in CI
- Fix backup and undo operations to use absolute paths
- Increase performance test timeout for CI compatibility

## [0.3.0] - 2025-01-13

### Added
- Configuration file support with YAML
- Logging support using tracing crate
- Performance benchmarks and troubleshooting guide
- Coverage reporting
- Automated changelog generation
- Library documentation

### Changed
- Update CLI to use rustscout crate
- Improve error handling
- Update dependencies

### Fixed
- Fix rustscout-cli to use local path for rustscout dependency
- Fix ignore patterns
- Fix code formatting

## [0.2.0] - 2025-01-13

### Added
- Changelog generation script
- Release workflow improvements
- Performance benchmarks
- Troubleshooting guide
- Coverage reporting

### Changed
- Update release workflow paths
- Improve asset upload process

### Fixed
- Fix code formatting
- Fix release workflow to build correct binary

### Added - 2025-01-13
- Initial release of rustscout and rustscout-cli
- High-performance concurrent file searching
- Support for simple string and regex patterns
- File filtering by extension and ignore patterns
- Stats-only mode for summary output
- Cross-platform support (Windows, macOS, Linux)
- Comprehensive documentation with .NET comparisons
- Benchmark suite for performance testing
- CI/CD pipeline with cross-platform testing
- Code coverage reporting with Codecov integration

### Changed
- Renamed project from RTrace to rustscout

### Fixed
- None

[Unreleased]: https://github.com/willibrandon/rustscout/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/willibrandon/rustscout/compare/v0.3.0...v1.0.0
[0.3.0]: https://github.com/willibrandon/rustscout/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/willibrandon/rustscout/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/willibrandon/rustscout/releases/tag/v0.1.0 