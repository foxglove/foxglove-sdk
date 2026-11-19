"""Hold the latest Twist command and repeat it ahead of a chassis watchdog.

Foxglove's Teleop panel publishes ``geometry_msgs/msg/Twist``. The Dexmate chassis
subscriber (``dexcontrol-rosbridge`` ``scripts/subscribe_chassis_vel.py``) listens
for ``geometry_msgs/msg/TwistStamped`` and zeros the base when no message arrives
for ``CHASSIS_COMMAND_WATCHDOG_S`` (100 ms). Teleop's default rate is 1 Hz, so a
one-shot conversion still lets the base stop between panel messages.

``CommandHold`` remembers the latest non-zero command and hands it back on every
poll until ``command_timeout_s`` passes with no new input. A zero command, or that
timeout, produces a single stop sample and then goes idle.
"""

from __future__ import annotations

import math
from dataclasses import dataclass

# dexcontrol-rosbridge scripts/subscribe_chassis_vel.py zeros the base when no
# TwistStamped arrives within this window. The repeat period must be shorter.
CHASSIS_COMMAND_WATCHDOG_S = 0.1

# 30 Hz is about 33 ms, comfortably inside the 100 ms chassis watchdog.
DEFAULT_REPEAT_HZ = 30.0

# Longer than the Teleop panel's default 1 Hz publish period, so a held button
# is not treated as a dropped link between panel messages.
DEFAULT_COMMAND_TIMEOUT_S = 2.0


@dataclass(frozen=True)
class TwistSample:
    """Velocity fields copied from a Twist or into a TwistStamped."""

    linear_x: float = 0.0
    linear_y: float = 0.0
    linear_z: float = 0.0
    angular_x: float = 0.0
    angular_y: float = 0.0
    angular_z: float = 0.0

    def is_stopped(self) -> bool:
        return (
            self.linear_x == 0.0
            and self.linear_y == 0.0
            and self.linear_z == 0.0
            and self.angular_x == 0.0
            and self.angular_y == 0.0
            and self.angular_z == 0.0
        )


STOPPED = TwistSample()


class CommandHold:
    """Latch the latest teleop command and decide what a repeat tick should send."""

    def __init__(self, command_timeout_s: float) -> None:
        if not math.isfinite(command_timeout_s) or command_timeout_s <= 0.0:
            raise ValueError("command_timeout_s must be a positive finite number")
        self._command_timeout_s = command_timeout_s
        self._command: TwistSample | None = None
        self._last_rx_s: float | None = None
        self._latched = False

    @property
    def command_timeout_s(self) -> float:
        return self._command_timeout_s

    @property
    def is_latched(self) -> bool:
        return self._latched

    def on_twist(self, sample: TwistSample, now_s: float) -> TwistSample:
        """Record a teleop message and return the sample to publish immediately.

        A zero sample clears the latch. The caller publishes that one stop, and
        later polls stay idle so a released button does not keep the topic hot.
        """
        self._last_rx_s = now_s
        if sample.is_stopped():
            self._command = None
            self._latched = False
            return STOPPED
        self._command = sample
        self._latched = True
        return sample

    def poll(self, now_s: float) -> TwistSample | None:
        """Return the latched command, one stop after the hold expires, or nothing.

        ``None`` means the relay should not publish on this tick.
        """
        if not self._latched or self._command is None or self._last_rx_s is None:
            return None
        elapsed = now_s - self._last_rx_s
        # A backwards clock step (sim time reset) keeps the latch instead of stopping.
        if elapsed < 0.0 or elapsed <= self._command_timeout_s:
            return self._command
        self._command = None
        self._latched = False
        return STOPPED
