from rest_framework.request import Request
from rest_framework.response import Response
from rest_framework.views import APIView


class MeView(APIView):
    def get(self, request: Request) -> Response:
        user = request.user
        return Response({"id": user.pk, "username": user.get_username(), "email": user.email})
