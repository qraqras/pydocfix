def missing_return(value) -> int:
    """Return a value.

    Args:
        value: Input value.
    """
    return value


def summary_only_return() -> int:
    """Return a value."""
    return 1


def summary_only_yield():
    """Yield values."""
    yield 1


def extra_return():
    """Do nothing.

    Returns:
        int: A value.
    """
    return None


def missing_raises(value):
    """Fail loudly.

    Args:
        value: Input value.
    """
    raise ValueError


def summary_only_raises():
    """Fail loudly."""
    raise ValueError


def extra_raises():
    """Do nothing.

    Raises:
        ValueError: Never raised.
    """
    return None
