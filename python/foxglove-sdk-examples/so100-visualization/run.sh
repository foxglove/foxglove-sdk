python roscon_demo.py \
  --leader.port /dev/ttyACM0 \
  --leader.id foxglove_leader \
  --follower.port /dev/ttyACM1 \
  --follower.id foxglove_follower \
  --leader.env_cam_id 9 \
  --leader.wrist_cam_id 4
  #--output.write_mcap
