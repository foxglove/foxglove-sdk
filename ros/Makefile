ROS_DISTRIBUTIONS := humble jazzy kilted rolling
FOXGLOVE_BRIDGE_REMOTE_ACCESS := ON
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
	rm -rf build dist install log

.PHONY: deps
deps:
	rosdep install -y \
		--from-paths src \
		--ignore-src

.PHONY: build
build:
	colcon build --cmake-args \
		-DFOXGLOVE_BRIDGE_REMOTE_ACCESS=$(FOXGLOVE_BRIDGE_REMOTE_ACCESS) \
		$(if $(FOXGLOVE_CPP_SDK_DIR),-DFETCHCONTENT_SOURCE_DIR_FOXGLOVE_SDK=$(FOXGLOVE_CPP_SDK_DIR))

.PHONY: test
test:
	GTEST_REPEAT=$(GTEST_REPEAT) colcon test
	colcon test-result --all --verbose

# Build a .deb package from the colcon install tree.
# Requires: the install tree must already exist (run docker-build-<distro> first).
FOXGLOVE_BRIDGE_VERSION := $(shell xmllint --xpath 'string(//version)' src/foxglove_bridge/package.xml)
FOXGLOVE_MSGS_VERSION := $(shell xmllint --xpath 'string(//version)' src/foxglove_msgs/package.xml)
DEB_ARCH := $(shell dpkg --print-architecture 2>/dev/null || echo amd64)

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
		make FOXGLOVE_BRIDGE_REMOTE_ACCESS=$(FOXGLOVE_BRIDGE_REMOTE_ACCESS) \
		$(if $(FOXGLOVE_CPP_SDK_DIR),FOXGLOVE_CPP_SDK_DIR=$(FOXGLOVE_CPP_SDK_DIR)) \
		build

.PHONY: docker-test-$(1)
docker-test-$(1):
	docker run --rm $(DOCKER_RUN_ARGS) -v $(PWD)/..:/sdk foxglove-sdk-ros-$(1) make GTEST_REPEAT=$(GTEST_REPEAT) test

.PHONY: docker-sh-$(1)
docker-sh-$(1):
	docker run --rm -it $(DOCKER_RUN_ARGS) -v $(PWD)/..:/sdk foxglove-sdk-ros-$(1)

.PHONY: deb-bridge-$(1)
deb-bridge-$(1): install/foxglove_bridge/lib/libfoxglove_bridge_component.so
	rm -rf $(CURDIR)/debian_pkg
	mkdir -p $(CURDIR)/debian_pkg/opt/ros/$(1)
	cp -a install/foxglove_bridge/. $(CURDIR)/debian_pkg/opt/ros/$(1)/
	mkdir -p $(CURDIR)/debian_pkg/DEBIAN
	{ \
	  echo 'Package: ros-$(1)-foxglove-bridge'; \
	  echo 'Version: $(FOXGLOVE_BRIDGE_VERSION)'; \
	  echo 'Architecture: $(DEB_ARCH)'; \
	  echo 'Maintainer: Foxglove <ros-tooling+foxglove_bridge@foxglove.dev>'; \
	  echo 'Depends: ros-$(1)-rclcpp, ros-$(1)-rclcpp-components, ros-$(1)-rosgraph-msgs, ros-$(1)-std-msgs, ros-$(1)-resource-retriever'; \
	  echo 'Description: Foxglove Bridge for ROS 2'; \
	  echo ' WebSocket bridge connecting ROS 2 to Foxglove.'; \
	} > $(CURDIR)/debian_pkg/DEBIAN/control
	mkdir -p dist
	dpkg-deb --build $(CURDIR)/debian_pkg dist/ros-$(1)-foxglove-bridge_$(FOXGLOVE_BRIDGE_VERSION)_$(DEB_ARCH).deb
	rm -rf $(CURDIR)/debian_pkg

.PHONY: deb-msgs-$(1)
deb-msgs-$(1): install/foxglove_msgs/share/foxglove_msgs/package.xml
	rm -rf $(CURDIR)/debian_pkg
	mkdir -p $(CURDIR)/debian_pkg/opt/ros/$(1)
	cp -a install/foxglove_msgs/. $(CURDIR)/debian_pkg/opt/ros/$(1)/
	mkdir -p $(CURDIR)/debian_pkg/DEBIAN
	{ \
	  echo 'Package: ros-$(1)-foxglove-msgs'; \
	  echo 'Version: $(FOXGLOVE_MSGS_VERSION)'; \
	  echo 'Architecture: $(DEB_ARCH)'; \
	  echo 'Maintainer: Foxglove <ros-tooling@foxglove.dev>'; \
	  echo 'Depends: ros-$(1)-rosidl-default-runtime, ros-$(1)-builtin-interfaces, ros-$(1)-geometry-msgs'; \
	  echo 'Description: Foxglove message definitions for ROS 2'; \
	  echo ' Message types supported by Foxglove visualization.'; \
	} > $(CURDIR)/debian_pkg/DEBIAN/control
	mkdir -p dist
	dpkg-deb --build $(CURDIR)/debian_pkg dist/ros-$(1)-foxglove-msgs_$(FOXGLOVE_MSGS_VERSION)_$(DEB_ARCH).deb
	rm -rf $(CURDIR)/debian_pkg

.PHONY: deb-$(1)
deb-$(1): deb-bridge-$(1) deb-msgs-$(1)

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

.PHONY: deb
deb: deb-rolling

.PHONY: deb-bridge
deb-bridge: deb-bridge-rolling

.PHONY: deb-msgs
deb-msgs: deb-msgs-rolling

.PHONY: lint
lint:
	find src/ -name '*.cpp' -o -name '*.hpp' -o -name '*.h' -o -name '*.cc' -o -name '*.inl' \
		| xargs clang-format --dry-run -Werror

.PHONY: fix
fix:
	find src/ -name '*.cpp' -o -name '*.hpp' -o -name '*.h' -o -name '*.cc' -o -name '*.inl' \
		| xargs clang-format -i
