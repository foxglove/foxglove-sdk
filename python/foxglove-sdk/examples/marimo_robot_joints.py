import marimo

__generated_with = "0.13.0"
app = marimo.App(width="full")


@app.cell
def _(mo):
    mo.md(
        """
        # Foxglove SDK — Marimo Robot Joint Visualization

        This notebook demonstrates how to use the Foxglove SDK's marimo integration to
        visualize simulated robot joint data. It generates sinusoidal joint positions for
        a 6-DOF robot arm and displays them in an embedded Foxglove viewer.
        """
    )
    return


@app.cell
def _():
    import math

    import foxglove
    from foxglove.marimo import MarimoBuffer
    from foxglove.schemas import JointState, JointStates, Timestamp

    return JointState, JointStates, MarimoBuffer, Timestamp, foxglove, math


@app.cell
def _(JointState, JointStates, MarimoBuffer, Timestamp, foxglove, math):
    # Joint configuration for a 6-DOF robot arm
    JOINT_NAMES = [
        "shoulder_pan",
        "shoulder_lift",
        "elbow",
        "wrist_1",
        "wrist_2",
        "wrist_3",
    ]

    # Sinusoidal parameters per joint: (amplitude_deg, frequency_hz, phase_offset_rad)
    JOINT_PARAMS = [
        (90.0, 0.2, 0.0),
        (45.0, 0.3, math.pi / 4),
        (60.0, 0.25, math.pi / 2),
        (30.0, 0.4, math.pi / 3),
        (20.0, 0.5, math.pi / 6),
        (15.0, 0.6, math.pi),
    ]

    DURATION_SEC = 10.0
    SAMPLE_RATE_HZ = 50.0

    # Create the marimo buffer
    buf = MarimoBuffer()

    # Generate sinusoidal joint data
    num_samples = int(DURATION_SEC * SAMPLE_RATE_HZ)
    dt = 1.0 / SAMPLE_RATE_HZ

    for i in range(num_samples):
        t = i * dt
        sec = int(t)
        nsec = int((t - sec) * 1e9)

        joints = []
        for name, (amplitude_deg, freq_hz, phase) in zip(JOINT_NAMES, JOINT_PARAMS):
            amplitude_rad = math.radians(amplitude_deg)
            position = amplitude_rad * math.sin(2.0 * math.pi * freq_hz * t + phase)
            velocity = (
                amplitude_rad
                * 2.0
                * math.pi
                * freq_hz
                * math.cos(2.0 * math.pi * freq_hz * t + phase)
            )

            joints.append(
                JointState(
                    name=name,
                    position=position,
                    velocity=velocity,
                )
            )

        foxglove.log(
            "/joint_states",
            JointStates(
                timestamp=Timestamp(sec=sec, nsec=nsec),
                joints=joints,
            ),
            log_time=sec * 1_000_000_000 + nsec,
        )

    # Display the Foxglove viewer
    widget = buf.show(height=600)
    widget
    return


@app.cell
def _(mo):
    mo.md(
        """
        ## Interactive Joint Control

        Use the sliders below to set each joint's position directly.
        The Foxglove viewer will update reactively when any slider changes.
        """
    )
    return


@app.cell
def _(mo):
    slider_shoulder_pan = mo.ui.slider(
        start=-180, stop=180, value=0, step=1, label="shoulder_pan (deg)"
    )
    slider_shoulder_lift = mo.ui.slider(
        start=-90, stop=90, value=0, step=1, label="shoulder_lift (deg)"
    )
    slider_elbow = mo.ui.slider(
        start=-120, stop=120, value=0, step=1, label="elbow (deg)"
    )
    slider_wrist_1 = mo.ui.slider(
        start=-180, stop=180, value=0, step=1, label="wrist_1 (deg)"
    )
    slider_wrist_2 = mo.ui.slider(
        start=-180, stop=180, value=0, step=1, label="wrist_2 (deg)"
    )
    slider_wrist_3 = mo.ui.slider(
        start=-180, stop=180, value=0, step=1, label="wrist_3 (deg)"
    )

    mo.vstack(
        [
            slider_shoulder_pan,
            slider_shoulder_lift,
            slider_elbow,
            slider_wrist_1,
            slider_wrist_2,
            slider_wrist_3,
        ]
    )
    return (
        slider_elbow,
        slider_shoulder_lift,
        slider_shoulder_pan,
        slider_wrist_1,
        slider_wrist_2,
        slider_wrist_3,
    )


@app.cell
def _(
    JointState,
    JointStates,
    MarimoBuffer,
    Timestamp,
    foxglove,
    math,
    slider_elbow,
    slider_shoulder_lift,
    slider_shoulder_pan,
    slider_wrist_1,
    slider_wrist_2,
    slider_wrist_3,
):
    # Map slider values (degrees) to joint positions (radians)
    _joint_positions = {
        "shoulder_pan": math.radians(slider_shoulder_pan.value),
        "shoulder_lift": math.radians(slider_shoulder_lift.value),
        "elbow": math.radians(slider_elbow.value),
        "wrist_1": math.radians(slider_wrist_1.value),
        "wrist_2": math.radians(slider_wrist_2.value),
        "wrist_3": math.radians(slider_wrist_3.value),
    }

    interactive_buf = MarimoBuffer()

    foxglove.log(
        "/interactive_joint_states",
        JointStates(
            timestamp=Timestamp(0, 0),
            joints=[
                JointState(name=name, position=pos)
                for name, pos in _joint_positions.items()
            ],
        ),
        log_time=0,
    )

    interactive_widget = interactive_buf.show(height=600)
    interactive_widget
    return


@app.cell
def _():
    import marimo as mo

    return (mo,)


if __name__ == "__main__":
    app.run()
