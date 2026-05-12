def missing_args_section(value):
    """Build a value."""
    return value


def missing_args_section_in_structured_docstring(value):
    """Build a value.

    Returns:
        int: The value.
    """
    return value


def extra_args_section():
    """Build a value.

    Args:
        value: Extra parameter.
    """
    return 1


class Example:
    def receiver(self, value):
        """Build a value.

        Args:
            self: The receiver.
            value: The value.
        """
        return value


def vararg_marker(*items):
    """Collect values.

    Args:
        items: Values.
    """
    return items


def kwargs_expanded(**kwargs):
    """Build from keyword options.

    Args:
        timeout: Timeout in seconds.
        retries: Number of retry attempts.
    """
    return kwargs
