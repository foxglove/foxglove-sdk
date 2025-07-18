

.PHONY: all
all: deps build test

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
	colcon build

.PHONY: test
test:
	colcon test

ROS_DISTRIBUTIONS := humble iron jazzy kilted rolling

define generate_ros_targets
.PHONY: docker-build-$(1)
docker-build-$(1):
	docker build -t foxglove-sdk-ros-$(1) -f ../Dockerfile.ros \
		--build-arg ROS_DISTRIBUTION=$(1) \
		$(EXTRA_DOCKER_ARGS) ..

.PHONY: docker-sh-$(1)
docker-sh-$(1):
	docker run --rm -it -v $(PWD)/..:/sdk foxglove-sdk-ros-$(1)
endef
$(foreach distribution,$(ROS_DISTRIBUTIONS),$(eval $(call generate_ros_targets,$(strip $(distribution)))))

.PHONY: docker-build
docker-build: docker-build-rolling

.PHONY: docker-sh
docker-sh: docker-sh-rolling
