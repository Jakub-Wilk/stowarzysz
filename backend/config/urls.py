import django_eventstream
from django.conf import settings
from django.conf.urls.static import static
from django.urls import include, path
from drf_spectacular.views import SpectacularAPIView, SpectacularSwaggerView

urlpatterns = [
    path("api/health/", include("core.urls")),
    path("api/auth/", include("accounts.urls")),
    path("api/polls/", include("voting.urls")),
    path("api/secret-santa/", include("secretsanta.urls")),
    path("api/pacts/", include("pacts.urls")),
    path("api/ledger/", include("ledger.urls")),
    path("api/push/", include("push.urls")),
    path("api/events/", include(django_eventstream.urls)),
    path("api/schema/", SpectacularAPIView.as_view(), name="schema"),
    path("api/docs/", SpectacularSwaggerView.as_view(url_name="schema"), name="docs"),
    *static(settings.MEDIA_URL, document_root=settings.MEDIA_ROOT),  # no-op unless DEBUG
]
