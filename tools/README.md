# Bottlerocket Test Tools

This container image provides a few of the binary tools that we need to use in Bottlerocket test agents.
Included are the binary and the license files, which we can use in downstream container image builds.

## Building

From the root of the git repository (i.e. `..`), run `make tools -e TOOLS_IMAGE=bottlerocket-test-tools:mytag`.

- The built image will always be tagged as `bottlerocket-test-tools:latest`, but it will also be tagged with `TOOLS_IMAGE`.
- If you plan on pushing the image to a remote repo, you can set `TOOLS_IMAGE=my.repo.com/my-bottlerocket-test-tools:v0.1.0` (replace with your values). Then you can `docker push my.repo.com/my-bottlerocket-test-tools:v0.1.0`.

## Using

If you have a built version, whether it is local or you have pushed it, you can use it when building the rest of the images in this git repo:

```shell
make eks-resource-agent -e TOOLS_IMAGE=TOOLS_IMAGE=my.repo.com/my-bottlerocket-test-tools:v0.1.0
```

## Default

By default, `TOOLS_IMAGE` will reference a specific version tag at `public.ecr.aws/bottlerocket/bottlerocket-test-tools`.
