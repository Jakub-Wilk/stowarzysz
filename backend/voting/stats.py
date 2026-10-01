"""Per-user voting statistics. Closed polls only: open ones would leak hidden votes."""

from typing import Any

from django.contrib.auth import get_user_model
from django.db.models import Avg, Count, F, FloatField, Q, QuerySet
from django.db.models.fields.json import KeyTextTransform
from django.db.models.functions import Cast
from rest_framework import serializers

from voting.models import Poll

_PREFIX = "poll_participations__"
_COUNTED = Q(
    **{
        f"{_PREFIX}poll__status": Poll.Status.CLOSED,
        f"{_PREFIX}poll__kind": "score",  # only score votes have a numeric average
        f"{_PREFIX}ballot__isnull": False,
    }
)


class VotingStatsSerializer(serializers.Serializer):
    votes_cast = serializers.IntegerField()
    average_score = serializers.FloatField(allow_null=True)
    veto_count = serializers.IntegerField()
    veto_percent = serializers.FloatField()


def annotate_voting_stats(queryset: QuerySet) -> QuerySet:
    """Add the aggregates `voting_stats` reads, so a user list needs a single query."""
    return queryset.annotate(
        _votes_cast=Count(f"{_PREFIX}pk", filter=_COUNTED),
        _veto_count=Count(f"{_PREFIX}pk", filter=_COUNTED & Q(**{f"{_PREFIX}vetoed": True})),
        _average_score=Avg(
            Cast(KeyTextTransform("value", F(f"{_PREFIX}ballot")), FloatField()), filter=_COUNTED
        ),
    )


def voting_stats(user: Any) -> dict[str, Any]:
    if not hasattr(user, "_votes_cast"):
        user = annotate_voting_stats(get_user_model()._default_manager.filter(pk=user.pk)).get()
    votes = user._votes_cast
    vetoes = user._veto_count
    average = user._average_score
    return {
        "votes_cast": votes,
        "average_score": round(average, 2) if average is not None else None,
        "veto_count": vetoes,
        "veto_percent": round(vetoes / votes * 100, 1) if votes else 0.0,
    }
