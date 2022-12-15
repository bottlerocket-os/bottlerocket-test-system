# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

## [0.0.4] - 2022-12-15

### Added

- K8s workload testing agent [#669]
- Custom userdata for Bottlerocket agents [#683]
- NVIDIA workload test definition [#696]

### Changed

- `test_manager::status` improvements
- builder: `build()` error type `Send + Sync` [#680]
- Change `EksctlConfig` to camelCase [#702]

[#669]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/669
[#680]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/680
[#683]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/683
[#696]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/696
[#702]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/702

## [0.0.3] - 2022-11-02

### Added

- ECS cluster provider support for existing IAM instance profiles [#555]
- Support for session tokens within aws secrets [#564]
- TestManager support for custom status columns [#591]
- EKS cluster provider accepts `eksctl` configuration files [#447]
- Builder macro for templated values in an agents config [#537]
- Constant for TestSys version [#626]
- VSphere Cluster provider [#613]

### Changed

- EC2 provider uses a list of subnets to launch instances [#585]
- EC2 provider uses a list of instance types to launch instances [#602]
- CRD API group was changed to `testsys.system` [#633]
- TestSys namespace was shortened to `testsys` [#633]
- Test Manager's uninstall was updated to remove TestSys crds [#635]

### Removed

- Yamlgen was removed [#580]
- The `parse-duration` crate was removed [#607]

[#555]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/555
[#564]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/564
[#591]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/591
[#447]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/447
[#537]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/537
[#626]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/626
[#613]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/613
[#585]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/585
[#602]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/602
[#633]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/633
[#635]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/635
[#580]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/580
[#607]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/607

## [0.0.2] - 2022-08-31

### Added

- This changelog
- Uninstall functionality for the TestManager [#450]
- This includes all changes since 0.0.1

[#450]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/450

## [0.0.1] - 2022-06-17

### Added

- Everything! (Except this changelog)
- Released the bottlerocket-agent containers at https://gallery.ecr.aws/bottlerocket-test-system

[Unreleased]: https://github.com/bottlerocket-os/bottlerocket-test-system/compare/v0.0.1...develop
[0.0.1]: https://github.com/bottlerocket-os/bottlerocket-test-system/tree/v0.0.1
