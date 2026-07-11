# Changelog

## [0.7.0] - 2025-07-11

### Added
- Self-serve license generation: users can enter their email in the activation screen to receive a free license key automatically via a Cloudflare Worker → Keygen.sh integration
- `workers/get-license.js` — Cloudflare Worker that creates Keygen users + licenses on demand

### Changed
- Activation screen now shows two options: enter existing key or request a free one via email
- License footer link updated to "Manage your license"

## [0.6.2] - 2025-07-11

### Fixed
- GitHub Actions workflow: duplicated `jobs` key in `build.yml` merged into a single block

## [0.6.1] - 2025-06-?

### Fixed
- Minor bug fixes

## [0.6.0] - 2025-06-?

### Changed
- Architecture migration from `Meeting` to `Session` core data model

## [0.5.5] - 2025-05-?

### Fixed
- Various bug fixes

## [0.5.4] - 2025-05-?

### Fixed
- Various bug fixes

## [0.5.3] - 2025-05-?

### Changed
- Updated dependencies

## [0.5.2] - 2025-05-?

### Fixed
- Various bug fixes

## [0.5.1] - 2025-05-?

### Fixed
- CI workflow fixes

## [0.4.1] - 2025-04-?

### Changed
- README improvements

## [0.4.0] - 2025-04-?

### Added
- Initial license validation via Keygen.sh
- Tauri updater integration with Keygen.sh releases
- GitHub Actions workflow for building and publishing to Keygen.sh

## [0.3.0] - 2025-03-?

### Added
- macOS build support
- Fedora (.rpm) build support

## [0.2.0] - 2025-03-?

### Added
- Windows build support
- Initial project setup with Tauri 2 + React + TypeScript
