from django.http import HttpRequest, HttpResponse, JsonResponse
from rest_framework.exceptions import AuthenticationFailed
from rest_framework.request import Request
from rest_framework_simplejwt.authentication import JWTAuthentication

EVENTS_PATH = "/api/events/"


class EventStreamJWTMiddleware:
    """Authenticate the SSE endpoint with `Authorization: Bearer <access token>`.

    django-eventstream is a plain Django view and only knows about `request.user`, so DRF's JWT
    authentication never runs for it. Browsers' native EventSource cannot set headers; use a
    fetch-based client such as @microsoft/fetch-event-source on the frontend.
    """

    def __init__(self, get_response):
        self.get_response = get_response

    def __call__(self, request: HttpRequest) -> HttpResponse:
        if request.path.startswith(EVENTS_PATH) and request.method != "OPTIONS":
            try:
                result = JWTAuthentication().authenticate(Request(request))
            except AuthenticationFailed as exc:
                return JsonResponse({"detail": str(exc.detail)}, status=401)
            if result is None:
                return JsonResponse(
                    {"detail": "Authentication credentials were not provided."}, status=401
                )
            request.user = result[0]  # ty: ignore[unresolved-attribute]
        return self.get_response(request)
