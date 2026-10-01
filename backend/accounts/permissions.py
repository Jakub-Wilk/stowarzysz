from rest_framework.permissions import BasePermission
from rest_framework.request import Request
from rest_framework.views import APIView


class IsSuperuser(BasePermission):
    """Account management is for superusers only."""

    def has_permission(self, request: Request, view: APIView) -> bool:
        user = request.user
        return bool(user and user.is_authenticated and user.is_superuser)
