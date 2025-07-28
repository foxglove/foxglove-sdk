

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
build: cpp-sdk
	colcon build --cmake-args -DLOCAL_SDK_BUILD=$(LOCAL_SDK_BUILD)

.PHONY: build-msgs
build-msgs:
	colcon build --packages-select foxglove_msgs

.PHONY: cpp-sdk
cpp-sdk:
ifeq ($(LOCAL_SDK_BUILD), ON)
	$(info Building Foxlove C++ SDK from local source tree)
	make -C ../cpp
else
	$(info Build will use pre-built Foxglove C++ SDK binaries)
endif

.PHONY: build-bridge
build-bridge: cpp-sdk
	colcon build --packages-select foxglove_bridge --cmake-args -DLOCAL_SDK_BUILD=$(LOCAL_SDK_BUILD) --event-handlers console_direct+

.PHONY: test
test: build
	colcon test
	colcon test-result --all --verbose

.PHONY: test-msgs
test-msgs: build-msgs
	colcon test --packages-select foxglove_msgs
	colcon test-result --all --verbose

.PHONY: test-bridge
test-bridge: build-bridge
	colcon test --packages-select foxglove_bridge
	colcon test-result --all --verbose

ROS_DISTRIBUTIONS := humble iron jazzy kilted rolling
LOCAL_SDK_BUILD := OFF

define generate_ros_targets
.PHONY: docker-build-$(1)
docker-build-$(1):
	docker build -t foxglove-sdk-ros-$(1) -f ../Dockerfile.ros \
		--build-arg ROS_DISTRIBUTION=$(1) \
		--build-arg LOCAL_SDK_BUILD=$(LOCAL_SDK_BUILD) \
		--build-arg BUILD_TARGET=build \
		$(EXTRA_DOCKER_ARGS) ..

.PHONY: docker-build-msgs-$(1)
docker-build-msgs-$(1):
	docker build -t foxglove-sdk-ros-$(1) -f ../Dockerfile.ros \
		--build-arg ROS_DISTRIBUTION=$(1) \
		--build-arg BUILD_TARGET=build-msgs \
		$(EXTRA_DOCKER_ARGS) ..

.PHONY: docker-build-bridge-$(1)
docker-build-bridge-$(1):
	docker build -t foxglove-sdk-ros-$(1) -f ../Dockerfile.ros \
		--build-arg ROS_DISTRIBUTION=$(1) \
		--build-arg LOCAL_SDK_BUILD=$(LOCAL_SDK_BUILD) \
		--build-arg BUILD_TARGET=build-bridge \
		$(EXTRA_DOCKER_ARGS) ..

.PHONY: docker-sh-$(1)
docker-sh-$(1):
	docker run --rm -it -v $(PWD)/..:/sdk foxglove-sdk-ros-$(1)

endef
$(foreach distribution,$(ROS_DISTRIBUTIONS),$(eval $(call generate_ros_targets,$(strip $(distribution)))))

.PHONY: docker-build
docker-build-msgs: docker-build-rolling

.PHONY: docker-build-msgs
docker-build-msgs: docker-build-msgs-rolling

.PHONY: docker-build-bridge
docker-build-bridge: docker-build-bridge-rolling

.PHONY: docker-sh
docker-sh: docker-sh-rolling
