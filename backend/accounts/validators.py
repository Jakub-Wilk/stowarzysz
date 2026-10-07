from django.core.validators import RegexValidator
from django.utils.deconstruct import deconstructible


@deconstructible
class UsernameValidator(RegexValidator):
    """Django's unicode username rules plus single spaces between words (none at the ends)."""

    regex = r"^[\w.@+-]+(?: [\w.@+-]+)*\Z"
    message = (
        "Nazwa może zawierać litery, cyfry oraz znaki @ . + - _ "
        "i pojedyncze spacje (nie na początku ani na końcu)."
    )
    flags = 0
