from django.contrib.auth.models import AbstractUser


class User(AbstractUser):
    """Custom user model from day one; extend it here instead of migrating away from auth.User."""
