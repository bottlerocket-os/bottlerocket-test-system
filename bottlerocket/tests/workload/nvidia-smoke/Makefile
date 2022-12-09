.DEFAULT_GOAL:=publish

#### Variables
PUBLISH_IMAGES_REGISTRY ?=
TAG ?= latest
REPOSITORY ?= gpu-tests

#### Format the registry portion of the image tag, if needed
REGISTRY ?=
ifdef PUBLISH_IMAGES_REGISTRY
REGISTRY = "$(PUBLISH_IMAGES_REGISTRY)/"
endif

#### Actual build targets
build:
ifndef REGISTRY
$(error "The PUBLISH_IMAGES_REGISTRY value must be provided (e.g. PUBLISH_IMAGES_REGISTRY=861807767978.dkr.ecr.us-east-2.amazonaws.com make)")
endif
	docker buildx build --platform linux/arm64,linux/amd64 --tag $(REGISTRY)$(REPOSITORY):$(TAG) .

publish:
	docker buildx build --push --platform linux/arm64,linux/amd64 --tag $(REGISTRY)$(REPOSITORY):$(TAG) .

x86_64:
	docker buildx build --platform linux/amd64 --tag $(REGISTRY)$(REPOSITORY):$(TAG)-x86_64 .

aarch64:
	docker buildx build --platform linux/arm64 --tag $(REGISTRY)$(REPOSITORY):$(TAG)-aarch64 .
