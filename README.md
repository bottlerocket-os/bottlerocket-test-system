# Bottlerocket Test System

A system for testing Bottlerocket.
To learn more about how it works, see the [design](docs/DESIGN.md) document.

## Overview

The system consists of a command line interface, Kubernetes controller, custom resource definition (CRD) objects and containers that allow you to create resources and run tests.
You install TestSys into a cluster of your choice, we call this the *TestSys cluster*.
When running a test, resource agents create an external cluster where we run Bottlerocket instances and run tests.
This is called an *external cluster*.

### Project Status

🚧 👷

The project is in active pre-release development.
Eventually we plan to publish container images and other aspects of the system, but we aren't quite there yet.
We also are not quite ready for external contributions, but we are happy to respond to issues and discussions.

## Quickstart

See our [QUICKSTART](docs/QUICKSTART.md) for a walk through of compiling, deploying, and running a quick test.

More detailed quickstart guides can be found in the main [Bottlerocket](https://github.com/bottlerocket-os/bottlerocket/) repo.
See the `QUICKSTART-*.md` files for information on the various deployment targets.

## Development

See the [Developer Guide for TestSys](docs/DEVELOPER.md) for an introduction to the framework and start up guide.

### Project Structure

- `testsys-model` is the root dependency. It includes the CRDs and clients for interacting with them.
- `controller` contains the Kubernetes controller responsible for running resource and test pods.
- `agent` contains libraries with the traits and harnesses for creating test and resource agents.
- `bottlerocket/agents` contains the implementations of the test and resource traits that we use for Bottlerocket testing.

The `testsys-model`, `agents` and `controller` crates are general-purpose, and define the TestSys system.
It is possible to use these libraries and controller for testing purposes other than Bottlerocket.

The `bottlerocket/agents` crates are more specialized to Bottlerocket's testing use cases.

## Security

See [CONTRIBUTING](CONTRIBUTING.md#security-issue-notifications) for more information.

## License

This project is dual licensed under either the Apache-2.0 License or the MIT license, your choice.
