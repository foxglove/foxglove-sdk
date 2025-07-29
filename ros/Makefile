ROS_DISTRIBUTIONS := humble iron jazzy kilted rolling
USE_LOCAL_PREBUILT_SDK := OFF
BUILD_SDK := OFF

# Automatically set USE_LOCAL_PREBUILT_SDK=ON if BUILD_SDK=ON
ifeq ($(BUILD_SDK), ON)
USE_LOCAL_PREBUILT_SDK := ON
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
build: cpp-sdk
	colcon build --cmake-args -DUSE_LOCAL_PREBUILT_SDK=$(USE_LOCAL_PREBUILT_SDK)

.PHONY: build-msgs
build-msgs:
	colcon build --packages-select foxglove_msgs

.PHONY: cpp-sdk
cpp-sdk:
ifeq ($(BUILD_SDK), ON)
	$(info Building Foxlove C++ SDK from local source tree)
	make -C ../cpp
endif

ifeq ($(USE_LOCAL_PREBUILT_SDK), ON)
	$(info Using pre-built Foxglove C++ SDK binaries from local source tree)
else
	$(info Build will use pre-built Foxglove C++ SDK binaries)
endif

.PHONY: build-bridge
build-bridge: cpp-sdk
	colcon build --packages-select foxglove_bridge --cmake-args -DUSE_LOCAL_PREBUILT_SDK=$(USE_LOCAL_PREBUILT_SDK) --event-handlers console_direct+

.PHONY: test
test:
	colcon test
	colcon test-result --all --verbose

define generate_ros_targets
.PHONY: docker-build-container-$(1)
docker-build-container-$(1):
	docker build -t foxglove-sdk-ros-$(1) -f ../Dockerfile.ros \
		--build-arg ROS_DISTRIBUTION=$(1) \
		$(EXTRA_DOCKER_ARGS) ..

.PHONY: docker-build-$(1)
docker-build-$(1):
	docker run --rm -v $(PWD):/sdk/ros foxglove-sdk-ros-$(1) make USE_LOCAL_PREBUILT_SDK=$(USE_LOCAL_PREBUILT_SDK) build

.PHONY: docker-build-msgs-$(1)
docker-build-msgs-$(1):
	docker run --rm -v $(PWD):/sdk/ros foxglove-sdk-ros-$(1) make build-msgs

.PHONY: docker-build-bridge-$(1)
docker-build-bridge-$(1):
	docker run --rm -v $(PWD):/sdk/ros foxglove-sdk-ros-$(1) make USE_LOCAL_PREBUILT_SDK=$(USE_LOCAL_PREBUILT_SDK) build-bridge

.PHONY: docker-test-$(1)
docker-test-$(1):
	docker run --rm -v $(PWD):/sdk/ros foxglove-sdk-ros-$(1) make test

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
