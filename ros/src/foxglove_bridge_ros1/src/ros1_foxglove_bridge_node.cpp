#include <ros/ros.h>

#include <foxglove_bridge_ros1/ros1_foxglove_bridge.hpp>

int main(int argc, char** argv) {
  ros::init(argc, argv, "foxglove_bridge");
  ros::NodeHandle nh;
  ros::NodeHandle privateNh("~");

  foxglove_bridge::Ros1FoxgloveBridge bridge(nh, privateNh);

  // Subscription and service callbacks come in on the SDK's own threads; the
  // ROS spinner only services subscription callbacks and timers.
  ros::AsyncSpinner spinner(4);
  spinner.start();
  ros::waitForShutdown();

  return 0;
}
