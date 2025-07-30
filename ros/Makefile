ROS_DISTRIBUTIONS := humble jazzy kilted rolling
USE_LOCAL_SDK := OFF

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
	colcon build --cmake-args -DUSE_LOCAL_SDK=$(USE_LOCAL_SDK)

.PHONY: test
test:
	colcon test
	colcon test-result --all --verbose

# --build-arg USER_UID=$(shell id -u) \
# --build-arg USER_GID=$(shell id -g) \

define generate_ros_targets
.PHONY: docker-build-container-$(1)
docker-build-container-$(1):
	docker build -t foxglove-sdk-ros-$(1) -f ../Dockerfile.ros \
		--build-arg ROS_DISTRIBUTION=$(1) \
		$(EXTRA_DOCKER_ARGS) ..

.PHONY: docker-build-$(1)
docker-build-$(1):
	docker run --rm -v $(PWD):/sdk/ros foxglove-sdk-ros-$(1) make USE_LOCAL_SDK=$(USE_LOCAL_SDK) build

.PHONY: docker-test-$(1)
docker-test-$(1):
	docker run --rm -v $(PWD):/sdk/ros foxglove-sdk-ros-$(1) make test

.PHONY: docker-sh-$(1)
docker-sh-$(1):
	docker run --rm -it -v $(PWD)/..:/sdk foxglove-sdk-ros-$(1)

endef
$(foreach distribution,$(ROS_DISTRIBUTIONS),$(eval $(call generate_ros_targets,$(strip $(distribution)))))

.PHONY: docker-build-container
docker-build-container: docker-build-container-rolling

.PHONY: docker-build
docker-build-msgs: docker-build-rolling

.PHONY: docker-sh
docker-sh: docker-sh-rolling
