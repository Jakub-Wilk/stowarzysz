"""Per-member records across settled pacts (confirmed claims only, so nothing is guessed).

Shown to administrators on a member's profile in the management panel."""

from collections import defaultdict
from typing import Any

from django.db.models import Sum
from rest_framework import serializers

from ledger.models import LedgerEntry
from pacts.models import OutcomeProposal


class KindStatsSerializer(serializers.Serializer):
    won = serializers.IntegerField()
    lost = serializers.IntegerField()
    draw = serializers.IntegerField()


class PactStatsSerializer(serializers.Serializer):
    """One member's record across settled pacts. Money is in minor units, PLN only."""

    won = serializers.IntegerField()
    lost = serializers.IntegerField()
    draw = serializers.IntegerField()
    money_won = serializers.IntegerField()
    money_lost = serializers.IntegerField()
    by_kind = serializers.DictField(child=KindStatsSerializer())


EMPTY: dict[str, Any] = {
    "won": 0,
    "lost": 0,
    "draw": 0,
    "money_won": 0,
    "money_lost": 0,
    "by_kind": {},
}


def all_pact_stats() -> dict[int, dict[str, Any]]:
    """Everyone's record in three queries, keyed by user id; members with no settled pacts are
    absent (use `EMPTY`). Void claims count for nobody."""
    record: dict[int, dict[str, dict[str, int]]] = defaultdict(
        lambda: defaultdict(lambda: {"won": 0, "lost": 0, "draw": 0})
    )
    claims = OutcomeProposal.objects.filter(state=OutcomeProposal.State.CONFIRMED).select_related(
        "pact"
    )
    for claim in claims:
        for user_id, verdict in claim.verdicts.items():
            if verdict in ("won", "lost", "draw"):
                record[int(user_id)][claim.pact.kind][verdict] += 1

    money = LedgerEntry.objects.filter(source_type="pact")
    won = {
        r["creditor"]: r["total"] for r in money.values("creditor").annotate(total=Sum("amount"))
    }
    lost = {r["debtor"]: r["total"] for r in money.values("debtor").annotate(total=Sum("amount"))}

    stats = {}
    for user_id in set(record) | set(won) | set(lost):
        by_kind = {kind: dict(counts) for kind, counts in record.get(user_id, {}).items()}
        stats[user_id] = {
            "won": sum(c["won"] for c in by_kind.values()),
            "lost": sum(c["lost"] for c in by_kind.values()),
            "draw": sum(c["draw"] for c in by_kind.values()),
            "money_won": won.get(user_id, 0),
            "money_lost": lost.get(user_id, 0),
            "by_kind": by_kind,
        }
    return stats
