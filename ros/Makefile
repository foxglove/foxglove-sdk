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
		-DCMAKE_EXPORT_COMPILE_COMMANDS=ON \
		-DFOXGLOVE_BRIDGE_REMOTE_ACCESS=$(FOXGLOVE_BRIDGE_REMOTE_ACCESS) \
		$(if $(FOXGLOVE_CPP_SDK_DIR),-DFETCHCONTENT_SOURCE_DIR_FOXGLOVE_SDK=$(FOXGLOVE_CPP_SDK_DIR))

.PHONY: test
test:
	GTEST_REPEAT=$(GTEST_REPEAT) colcon test
	colcon test-result --all --verbose

# Build .deb packages using bloom. These targets run inside the Docker container
# (via docker-deb-<distro>), where ROS and OS metadata are available.
BLOOM_BRIDGE_CMAKE_ARGS := \
	-DFETCHCONTENT_FULLY_DISCONNECTED=OFF \
	-DFOXGLOVE_BRIDGE_REMOTE_ACCESS=$(FOXGLOVE_BRIDGE_REMOTE_ACCESS) \
	$(if $(FOXGLOVE_CPP_SDK_DIR),-DFETCHCONTENT_SOURCE_DIR_FOXGLOVE_SDK=$(FOXGLOVE_CPP_SDK_DIR))

.PHONY: deb-bridge
deb-bridge:
	cd src/foxglove_bridge && bloom-generate rosdebian
	sed -i 's|dh_auto_configure --|dh_auto_configure -- $(BLOOM_BRIDGE_CMAKE_ARGS)|' \
		src/foxglove_bridge/debian/rules
	cd src/foxglove_bridge && fakeroot debian/rules binary
	mkdir -p dist && mv src/ros-*-foxglove-bridge*.deb dist/
	rm -rf src/foxglove_bridge/debian src/foxglove_bridge/.obj-*
	rm -f src/ros-*-foxglove-bridge*

.PHONY: deb-msgs
deb-msgs:
	cd src/foxglove_msgs && bloom-generate rosdebian
	cd src/foxglove_msgs && fakeroot debian/rules binary
	mkdir -p dist && mv src/ros-*-foxglove-msgs*.deb dist/
	rm -rf src/foxglove_msgs/debian src/foxglove_msgs/.obj-*
	rm -f src/ros-*-foxglove-msgs*

.PHONY: deb
deb: deb-bridge deb-msgs

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

.PHONY: docker-deb-bridge-$(1)
docker-deb-bridge-$(1): docker-build-image-$(1)
	docker run --rm $(DOCKER_RUN_ARGS) -v $(PWD)/..:/sdk foxglove-sdk-ros-$(1) \
		make FOXGLOVE_BRIDGE_REMOTE_ACCESS=$(FOXGLOVE_BRIDGE_REMOTE_ACCESS) \
		$(if $(FOXGLOVE_CPP_SDK_DIR),FOXGLOVE_CPP_SDK_DIR=$(FOXGLOVE_CPP_SDK_DIR)) \
		deb-bridge

.PHONY: docker-deb-msgs-$(1)
docker-deb-msgs-$(1): docker-build-image-$(1)
	docker run --rm $(DOCKER_RUN_ARGS) -v $(PWD)/..:/sdk foxglove-sdk-ros-$(1) \
		make deb-msgs

.PHONY: docker-deb-$(1)
docker-deb-$(1): docker-deb-bridge-$(1) docker-deb-msgs-$(1)

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

.PHONY: docker-deb
docker-deb: docker-deb-rolling

.PHONY: docker-deb-bridge
docker-deb-bridge: docker-deb-bridge-rolling

.PHONY: docker-deb-msgs
docker-deb-msgs: docker-deb-msgs-rolling

# Merge per-package compile_commands.json and remap container paths to host paths.
# Run after docker-build to get IDE support (clangd, etc).
.PHONY: compile-commands
compile-commands:
	jq -s 'add' build/*/compile_commands.json \
		| sed 's|/sdk|$(realpath $(CURDIR)/..)|g' > build/compile_commands.json

.PHONY: lint
lint:
	find src/ -name '*.cpp' -o -name '*.hpp' -o -name '*.h' -o -name '*.cc' -o -name '*.inl' \
		| xargs clang-format --dry-run -Werror

.PHONY: fix
fix:
	find src/ -name '*.cpp' -o -name '*.hpp' -o -name '*.h' -o -name '*.cc' -o -name '*.inl' \
		| xargs clang-format -i
