from foxglove import Context


def test_default_context_is_singleton() -> None:
    assert Context.default() is Context.default()


def test_context_is_distinct() -> None:
    assert Context() is not Context.default()
    assert Context() is not Context()
