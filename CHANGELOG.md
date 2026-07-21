# Changelog / 变更日志

This project follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) for user-visible changes. No formal version has been released.

本项目按 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 记录用户可见变更。目前没有正式发布版本。

## [Unreleased]

### Added

- Recoverable MOSS multispeaker transcription with loopback-only service configuration, background jobs, candidate review, speaker review, and structured export.
- English Creator Source Beta guidance and external Agent handoff instructions.
- Windows unsigned-candidate acceptance records.

### Changed

- Pull request CI now validates stacked branches as well as branches that target `main` directly.
- External Agent states distinguish waiting for claim, active processing, submitted results, and human review.

### Fixed

- Transcription result application now preserves later project edits and requires explicit replacement confirmation.
- Prepared transcription results can recover after an interrupted finalization step.
- Subtitle merge tests now wait for the asynchronous transcript refresh.

### Release status

- Source and unsigned local candidates remain development artifacts.
- No GitHub tag, prerelease, or formal Release exists.
- Windows 11, physical sleep/wake, historical binary upgrade, real MOSS media, and external Creator Beta acceptance remain incomplete.

[Unreleased]: https://github.com/ShawnSiao/siao-cut/compare/main...HEAD
