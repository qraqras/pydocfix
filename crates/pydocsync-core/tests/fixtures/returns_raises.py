def missing_return() -> int:
    """Return a value."""
    return 1


def extra_return():
    """Do nothing.

    Returns:
        int: A value.
    """
    return None


def missing_raises():
    """Fail loudly."""
    raise ValueError


def extra_raises():
    """Do nothing.

    Raises:
        ValueError: Never raised.
    """
    return None
