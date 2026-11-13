"""CommandHold keeps a 1 Hz Teleop stream inside the chassis watchdog."""

import ast
import unittest
from pathlib import Path

from twist_to_twist_stamped.hold import (
    CHASSIS_COMMAND_WATCHDOG_S,
    DEFAULT_COMMAND_TIMEOUT_S,
    DEFAULT_REPEAT_HZ,
    STOPPED,
    CommandHold,
    TwistSample,
)


class CommandHoldTest(unittest.TestCase):
    def test_idle_until_the_first_command(self):
        hold = CommandHold(DEFAULT_COMMAND_TIMEOUT_S)
        self.assertIsNone(hold.poll(0.0))
        self.assertFalse(hold.is_latched)

    def test_repeats_every_field_including_lateral_velocity(self):
        hold = CommandHold(DEFAULT_COMMAND_TIMEOUT_S)
        cmd = TwistSample(
            linear_x=0.2,
            linear_y=-0.1,
            linear_z=0.05,
            angular_x=0.01,
            angular_y=-0.02,
            angular_z=0.3,
        )
        self.assertEqual(hold.on_twist(cmd, now_s=1.0), cmd)
        self.assertTrue(hold.is_latched)
        self.assertEqual(hold.poll(1.5), cmd)

    def test_default_repeat_period_is_inside_the_chassis_watchdog(self):
        period_s = 1.0 / DEFAULT_REPEAT_HZ
        self.assertLess(period_s, CHASSIS_COMMAND_WATCHDOG_S)

    def test_one_hertz_teleop_is_repeated_inside_the_watchdog(self):
        """Teleop's default 1 Hz rate must still refresh the chassis faster than 100 ms."""
        hold = CommandHold(DEFAULT_COMMAND_TIMEOUT_S)
        cmd = TwistSample(linear_x=0.4, angular_z=-0.2)
        period_s = 1.0 / DEFAULT_REPEAT_HZ
        last_motion = None
        t = 0.0
        for teleop_tick in range(3):
            hold.on_twist(cmd, now_s=float(teleop_tick))
            while t < teleop_tick + 1.0:
                published = hold.poll(t)
                self.assertEqual(published, cmd)
                if last_motion is not None:
                    self.assertLessEqual(t - last_motion, CHASSIS_COMMAND_WATCHDOG_S)
                last_motion = t
                t += period_s

    def test_timeout_publishes_one_stop_then_idles(self):
        hold = CommandHold(0.5)
        cmd = TwistSample(linear_x=0.3)
        hold.on_twist(cmd, now_s=0.0)
        self.assertEqual(hold.poll(0.5), cmd)
        self.assertIs(hold.poll(0.5 + 1e-6), STOPPED)
        self.assertFalse(hold.is_latched)
        self.assertIsNone(hold.poll(1.0))
        self.assertIsNone(hold.poll(5.0))

    def test_stop_command_is_published_once_and_not_repeated(self):
        hold = CommandHold(DEFAULT_COMMAND_TIMEOUT_S)
        hold.on_twist(TwistSample(linear_x=1.0), now_s=0.0)
        self.assertIs(hold.on_twist(TwistSample(), now_s=0.2), STOPPED)
        self.assertFalse(hold.is_latched)
        self.assertIsNone(hold.poll(0.3))
        self.assertIsNone(hold.poll(3.0))

    def test_motion_after_a_stop_latches_again(self):
        hold = CommandHold(0.5)
        hold.on_twist(TwistSample(linear_x=1.0), now_s=0.0)
        hold.on_twist(STOPPED, now_s=0.1)
        resumed = TwistSample(linear_y=0.25, angular_z=-0.4)
        self.assertEqual(hold.on_twist(resumed, now_s=1.0), resumed)
        self.assertEqual(hold.poll(1.4), resumed)

    def test_backwards_clock_keeps_the_latched_command(self):
        hold = CommandHold(0.5)
        cmd = TwistSample(angular_z=0.2)
        hold.on_twist(cmd, now_s=10.0)
        self.assertEqual(hold.poll(9.0), cmd)

    def test_timeout_must_be_positive(self):
        for value in (0.0, -1.0, float("nan"), float("inf")):
            with self.assertRaises(ValueError):
                CommandHold(value)

    def test_node_module_parses(self):
        source = (
            Path(__file__).resolve().parents[1] / "twist_to_twist_stamped" / "node.py"
        )
        ast.parse(source.read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
