ARG ROS_DISTRIBUTION=rolling
FROM ros:$ROS_DISTRIBUTION-ros-base

RUN apt-get update

# Create foxglove user
ARG USERNAME=foxglove
ARG USER_UID=1005
ARG USER_GID=$USER_UID
RUN groupadd --gid $USER_GID $USERNAME \
    && useradd --uid $USER_UID --gid $USER_GID -m $USERNAME \
    && echo "$USERNAME ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers.d/$USERNAME

USER $USERNAME

# rosdep update must run as user
RUN rosdep update --include-eol-distros

# Set up the workspace
WORKDIR /ros
COPY --chown=$USER_UID:$USER_GID . /ros/src/foxglove_bridge

# Install ROS dependencies
RUN rosdep install -y \
    --from-paths src \
    --ignore-src

# Build bridge
RUN /bin/bash -c '. /opt/ros/$ROS_DISTRO/setup.bash && \
    colcon build --packages-select foxglove_bridge'

RUN <<EOF
# Write out bash script to source ROS setup script and wrap launch file
echo '#!/bin/bash
set -e
source /ros/install/setup.bash
exec ros2 launch foxglove_bridge foxglove_bridge_launch.xml "$@"' > /ros/entrypoint.sh

chmod +x /ros/entrypoint.sh
EOF


EXPOSE 8765

ENTRYPOINT ["./entrypoint.sh"]
