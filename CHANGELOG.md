# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

[Unreleased]: https://github.com/bottlerocket-os/bottlerocket-test-system/compare/v0.0.11...develop

## [0.0.11] - 2024-01-29

### Added

- vSphere: Delete conflicting vms/templates [#879]
- sonobuoy: Automatically delete namespace [#883]
- controller: Add flag to enable log archiving [#882]

[#879]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/879
[#882]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/882
[#883]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/883

[0.0.11]: https://github.com/bottlerocket-os/bottlerocket-test-system/tree/v0.0.11

## [0.0.10] - 2023-10-03

### Fixed

- eks-resource: Process paginated results from list-stacks ([#873])
- karpenter-ec2: Take cluster sg as a single string instead of sequence ([#874])

### Added

- ecs-provider: Add name tag to ec2 instances ([#875])

### Removed

- Remove extraneous default() calls ([#847])
- Removed indirect dependencies ([#848])

[#847]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/847
[#848]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/848
[#873]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/873
[#874]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/874
[#875]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/875

[0.0.10]: https://github.com/bottlerocket-os/bottlerocket-test-system/tree/v0.0.10

## [0.0.9] - 2023-09-13

### Fixed

- Increase sonobuoy status check timeout to 15 mins and fix image used in retries ([#868])

### Added

- Add EKS service endpoint override in the EKS resource agent ([#860])
- Set necessary environment variables to enable new K8s version cluster creation in metal and vsphere agents ([#866])
- Add option for EKS-A release manifest url and fetch EKS-A binary at runtime in metal and vsphere agents ([#867])

### Changed

- Remove `eksctl` build workaround ([#844])
- Remove `--force-cleanup` flag from eks-a invocation in metal and vsphere agents ([#851])
- Restrict IMDS on nodes launched by testsys-launcher ([#852])
- Build fixes and enhancements in the tools image ([#858])
- Rust crate dependency updates ([#862])
- Bump Bottlerocket SDK version to 0.34.1 ([#871])

[#844]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/844
[#851]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/851
[#852]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/852
[#858]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/858
[#860]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/860
[#862]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/862
[#866]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/866
[#867]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/867
[#868]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/868
[#871]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/871

[0.0.9]: https://github.com/bottlerocket-os/bottlerocket-test-system/tree/v0.0.9

## [0.0.8] - 2023-06-12

### Fixed

- controller: Add retry logic to controller if it's unable to find resources ([#816])
- karpenter: Remove extra { and } from ConfigMap username ([#818])
- Fix various typos and spelling errors ([#835])

### Changed

- doc: Add documentation for minimal iam permissions ([#775])
- Add Testsys-launcher ([#823], [#824], [#826], [#831])
- sonobuoy-test-agent: Add non-blocking-taints for control plane nodes ([#832])
- Add hello-testsys workload test definition ([#834])

[#775]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/775
[#816]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/816
[#818]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/818
[#823]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/823
[#824]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/824
[#826]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/826
[#831]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/831
[#832]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/832
[#834]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/834
[#835]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/835

[0.0.8]: https://github.com/bottlerocket-os/bottlerocket-test-system/tree/v0.0.8

## [0.0.7] - 2023-03-03

### Fixed

- The `get-secrets` command of `test-agent-cli` now works as expected ([#812])

### Added

- Add support for karpenter testing ([#803])

### Changed

- `StatusSnapshot` supports custom columns for tables ([#777])
- Support `--sonobuoy-image` flag in the sonobuoy agent ([#801])
- Openssl dependency has been removed in favor of rustls ([#766])

[#766]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/766
[#777]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/777
[#801]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/801
[#803]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/803
[#812]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/812

[0.0.7]: https://github.com/bottlerocket-os/bottlerocket-test-system/tree/v0.0.7

## [0.0.6] - 2023-03-03

### Fixed

- Add category field to allow getting all testsys objects ([#726])
- TestManager: Simplify code for `cargo make test` ([#742])
- TestManager: Block on uninstall for namespace ([#745])
- Added check to make sure that `metadata_url` ends with `/` ([#765])
- TestManager: Ensure no resources for uninstall ([#770])
- Agents: Fix snake case for EksctlConfig ([#744])

### Added

- ECS workload testing agent ([#725])
- Sample test config files ([#740], [#750], [#756], [#760])
- Sample Makefile.toml ([#751], [#761], [#771], [#772])
- Support for `assume_role` in workload agents ([#752])
- Metal k8s resource provider ([#773])

### Changed

- `run-instances` now uses IMDSv2 ([#753])
- Renamed the `model` crate to `testsys-model` ([#755])

### Removed

- `bottlerocket/testsys` ([#754], [#759])

[#726]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/726
[#725]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/725
[#742]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/742
[#740]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/740
[#744]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/744
[#745]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/745
[#750]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/750
[#751]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/751
[#752]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/752
[#753]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/753
[#754]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/754
[#755]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/755
[#756]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/756
[#759]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/759
[#760]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/760
[#761]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/761
[#765]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/765
[#770]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/770
[#771]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/771
[#772]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/772
[#773]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/773

[0.0.6]: https://github.com/bottlerocket-os/bottlerocket-test-system/tree/v0.0.6

## [0.0.5] - 2022-12-20

### Fixed

- TestManager: Show state for the current test ([#714])
- Fix calling `sonobuoy retrieve` too soon when sonobuoy tests results weren't ready ([#715])
- Wait for container instances to fully deregister before cleaning-up ([#716], [#720])

[#714]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/714
[#715]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/715
[#716]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/716
[#720]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/720

[0.0.5]: https://github.com/bottlerocket-os/bottlerocket-test-system/tree/v0.0.5

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

[0.0.4]: https://github.com/bottlerocket-os/bottlerocket-test-system/tree/v0.0.4

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

[0.0.3]: https://github.com/bottlerocket-os/bottlerocket-test-system/tree/v0.0.3

## [0.0.2] - 2022-08-31

### Added

- This changelog
- Uninstall functionality for the TestManager [#450]
- This includes all changes since 0.0.1

[#450]: https://github.com/bottlerocket-os/bottlerocket-test-system/pull/450

[0.0.2]: https://github.com/bottlerocket-os/bottlerocket-test-system/tree/v0.0.2

## [0.0.1] - 2022-06-17

### Added

- Everything! (Except this changelog)
- Released the bottlerocket-agent containers at https://gallery.ecr.aws/bottlerocket-test-system

[0.0.1]: https://github.com/bottlerocket-os/bottlerocket-test-system/tree/v0.0.1
