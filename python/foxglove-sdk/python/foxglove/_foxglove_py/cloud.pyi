class CloudSink:
    """
    A cloud sink for remote visualization and teleop.
    """

    def __init__(self) -> None: ...
    def stop(self) -> None:
        """Gracefully disconnect from the cloud sink, waiting for shutdown to complete."""
        ...
