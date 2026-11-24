from setuptools import find_packages, setup

package_name = "twist_to_twist_stamped"

setup(
    name=package_name,
    version="0.1.0",
    packages=find_packages(exclude=["test"]),
    data_files=[
        ("share/ament_index/resource_index/packages", ["resource/" + package_name]),
        ("share/" + package_name, ["package.xml"]),
        (
            "share/" + package_name + "/launch",
            ["launch/twist_to_twist_stamped.launch.py"],
        ),
    ],
    install_requires=["setuptools"],
    zip_safe=True,
    maintainer="Foxglove",
    maintainer_email="ros-tooling+foxglove_bridge@foxglove.dev",
    description=(
        "Republish geometry_msgs/msg/Twist as TwistStamped and repeat the last "
        "command so a chassis watchdog keeps seeing fresh setpoints."
    ),
    license="MIT",
    tests_require=["pytest"],
    entry_points={
        "console_scripts": [
            "twist_to_twist_stamped = twist_to_twist_stamped.node:main",
        ],
    },
)
