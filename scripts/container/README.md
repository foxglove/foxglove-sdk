# Container scripts

Utility scripts that run inside Docker containers (e.g., the netem sidecar). The sidecar containers use `python:3-alpine` so scripts are written in Python. `iproute2` (for `tc`) is installed at container startup.

These scripts are mounted into containers via Docker Compose volume binds and are not intended to be run directly on the host.
