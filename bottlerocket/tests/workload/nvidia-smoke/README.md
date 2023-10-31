# NVIDIA GPU Smoke tests

The image created by the Dockerfile in this folder compiles a few [CUDA samples](https://github.com/NVIDIA/cuda-samples/) for both `x86_64` and `aarch64`.
This container image can be used as a TestSys workload test to validate the Bottlerocket `*-nvidia` variants running on NVIDIA hosts.

## Build requirements

Due to missing packages needed for cross-compiling, and unexpected handling in the sample Makefile logic when compiling for one platform for another, the build for this image requires two Buildx hosts - one for each architecture.

In order to build the image for both architectures, you need to set up the `buildx` docker plugin.
[This guide describes](https://github.com/docker/buildx#installing) the installation process.

By default, the install of `docker buildx` uses the `docker` driver type.
This driver is not able to build for all platform targets we need.
You will need to run the following to create a new builder using the `docker-container` driver:

```bash
docker buildx create --use --bootstrap
docker buildx ls
```

You should see a `*` next to your newly created builder denoting it as the currently active builder to use.
If it is not, you can switch builder contexts by running:

```sh
docker buildx use $BUILDER_NAME
```

You then need to add another "context" to the builder for a second host that can build the other platform architecture.
If you are running these steps on an `amd64` host, you will need to add an `arm64` host to the builder, or vice versa.

First, verify you are able to access the remote host via SSH:

```sh
docker -H ssh://user@hostname info
```

The output from that command should show information about the Docker instance running on the remote host.
You can then add that host to your Buildx builder by running:

```sh
docker buildx create --name $BUILDER_NAME --append ssh://user@hostname
```

You must bootstrap the new builder.

```sh
docker buildx inspect --bootstrap --builder $BUILDER_NAME
```

Supported platforms can be verified by running:

```bash
docker buildx inspect | grep Platforms
```

The output from this command should show both `linux/amd64` and `linux/arm64` platforms.

By default, the image will be tagged `<PUBLISH_IMAGES_REGISTRY>/gpu-tests:<TAG>`, so make sure you already have a `gpu-tests` repository in your registry.
You can change the name of the repo by overriding the `PUBLISH_IMAGES_REGISTRY` env variable while building the image.

**NOTE:** This assumes that you have already configured your credentials to be able to perform a `docker push` to your registry.

## Building the image

To build the image for both `x86_64` and `aarch64`, run the following:

```sh
PUBLISH_IMAGES_REGISTRY=<YOUR_REGISTRY> make
```

The command will build, tag, and push the images to your `PUBLISH_IMAGES_REGISTRY` using the name `gpu-tests:<TAG>`.

Since this is a multi-arch image, you can use it in both `x86_64` and `aarch64` clusters.
