ROS_DISTRIBUTIONS := humble jazzy kilted rolling
USE_LOCAL_SDK := OFF
FOXGLOVE_CPP_SDK_DIR :=
EXTRA_DOCKER_ARGS :=
DOCKER_USER_ARGS :=
DOCKER_RUN_ARGS :=
GTEST_REPEAT := 1

ifeq ($(shell uname -s), Linux)
	DOCKER_USER_ARGS = --build-arg USER_UID=$(shell id -u) --build-arg USER_GID=$(shell id -g)
endif

# Detect rootless podman. In rootless mode, UID 0 inside the container maps to
# the host user's UID, so running as root avoids both permission issues on volume
# mounts and the slow id-mapped layer copies that --userns=keep-id requires.
ifneq ($(shell docker --version 2>&1 | grep -c podman),0)
	ifneq ($(shell docker info --format '{{.Host.Security.Rootless}}' 2>/dev/null),false)
		DOCKER_RUN_ARGS += --user 0:0
	endif
endif

.PHONY: all
all: deps build

.PHONY: clean
clean:
	rm -rf build install log

.PHONY: deps
deps:
	rosdep install -y \
		--from-paths src \
		--ignore-src

.PHONY: build
build:
	colcon build --cmake-args \
		-DUSE_LOCAL_SDK=$(USE_LOCAL_SDK) \
		$(if $(FOXGLOVE_CPP_SDK_DIR),-DFETCHCONTENT_SOURCE_DIR_FOXGLOVE_SDK=$(FOXGLOVE_CPP_SDK_DIR))

.PHONY: test
test:
	GTEST_REPEAT=$(GTEST_REPEAT) colcon test
	colcon test-result --all --verbose

define generate_ros_targets
.PHONY: docker-build-image-$(1)
docker-build-image-$(1):
	docker build -t foxglove-sdk-ros-$(1) -f ../Dockerfile.ros \
		--pull \
		--build-arg ROS_DISTRIBUTION=$(1) \
		$(DOCKER_USER_ARGS) \
		$(EXTRA_DOCKER_ARGS) ..

.PHONY: docker-build-$(1)
docker-build-$(1): docker-build-image-$(1)
	docker run --rm $(DOCKER_RUN_ARGS) -v $(PWD)/..:/sdk foxglove-sdk-ros-$(1) \
		make USE_LOCAL_SDK=$(USE_LOCAL_SDK) \
		$(if $(FOXGLOVE_CPP_SDK_DIR),FOXGLOVE_CPP_SDK_DIR=$(FOXGLOVE_CPP_SDK_DIR)) \
		build

.PHONY: docker-test-$(1)
docker-test-$(1):
	docker run --rm $(DOCKER_RUN_ARGS) -v $(PWD)/..:/sdk foxglove-sdk-ros-$(1) make GTEST_REPEAT=$(GTEST_REPEAT) test

.PHONY: docker-sh-$(1)
docker-sh-$(1):
	docker run --rm -it $(DOCKER_RUN_ARGS) -v $(PWD)/..:/sdk foxglove-sdk-ros-$(1)

endef
$(foreach distribution,$(ROS_DISTRIBUTIONS),$(eval $(call generate_ros_targets,$(strip $(distribution)))))

.PHONY: docker-build-image
docker-build-image: docker-build-image-rolling

.PHONY: docker-build
docker-build: docker-build-rolling

.PHONY: docker-test
docker-test: docker-test-rolling

.PHONY: docker-sh
docker-sh: docker-sh-rolling

.PHONY: lint
lint:
	find src/ -name '*.cpp' -o -name '*.hpp' -o -name '*.h' -o -name '*.cc' -o -name '*.inl' \
		| xargs clang-format --dry-run -Werror

.PHONY: fix
fix:
	find src/ -name '*.cpp' -o -name '*.hpp' -o -name '*.h' -o -name '*.cc' -o -name '*.inl' \
		| xargs clang-format -i
