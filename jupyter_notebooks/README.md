# Jupyter notebook examples

This directory contains notebook examples for the Foxglove embed integration.

## SO-101 JointStates Colab embed

- Notebook: `so101_jointstates_colab_embed.ipynb`
- Layout JSON: `layouts/so101_jointstates_layout.json`

The notebook shows how to:

1. Load the SO-101 URDF in a Foxglove 3D panel.
2. Publish sinusoidal `foxglove.JointStates` messages on `/joint_states`.
3. Animate the robot model using **joint-states-only** control mode (no TF publisher).
