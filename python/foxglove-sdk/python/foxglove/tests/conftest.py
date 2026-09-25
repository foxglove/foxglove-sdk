from importlib.util import find_spec

lerobot_installed = all(find_spec(name) for name in ("av", "pyarrow"))
collect_ignore = [] if lerobot_installed else ["test_lerobot.py"]
