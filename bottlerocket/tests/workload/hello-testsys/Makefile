.DEFAULT_GOAL:=publish

#### Variables
PUBLISH_IMAGES_REGISTRY ?=
TAG ?= latest
REPOSITORY ?= hello-testsys

#### Format the registry portion of the image tag, if needed
REGISTRY ?=
ifdef PUBLISH_IMAGES_REGISTRY
REGISTRY = "$(PUBLISH_IMAGES_REGISTRY)/"
endif

ifndef REGISTRY
$(error "The PUBLISH_IMAGES_REGISTRY value must be provided (e.g. PUBLISH_IMAGES_REGISTRY=861807767978.dkr.ecr.us-east-2.amazonaws.com make)")
endif

define build-image
	docker buildx build . -t $(REPOSITORY)-$1 --platform $2 --load
endef

define tag-image
	docker tag $(REPOSITORY)-$1 $(REGISTRY)$(REPOSITORY):$(TAG)-$1
endef

define push-image
	docker push $(REGISTRY)$(REPOSITORY):$(TAG)-$1
endef

#### Actual build targets
.PHONY: build
build: x86_64 aarch64

.PHONY: x86_64
x86_64:
	$(call build-image,$@,linux/amd64)
	$(call tag-image,x86_64)

.PHONY: aarch64
aarch64:
	$(call build-image,$@,linux/aarch64)
	$(call tag-image,aarch64)

.PHONY: push-platforms
push-platforms: build
	$(call push-image,x86_64)
	$(call push-image,aarch64)

.PHONY: create-manifest
create-manifest: push-platforms
	- docker manifest rm $(REGISTRY)$(REPOSITORY):$(TAG)
	docker manifest create \
		$(REGISTRY)$(REPOSITORY):$(TAG) \
		$(REGISTRY)$(REPOSITORY):$(TAG)-x86_64 \
		$(REGISTRY)$(REPOSITORY):$(TAG)-aarch64 \

.PHONY: publish
publish: create-manifest
	docker manifest push $(REGISTRY)$(REPOSITORY):$(TAG)
