"""Per-member records across settled pacts (confirmed claims only, so nothing is guessed)."""

from collections import defaultdict
from typing import Any

from django.contrib.auth import get_user_model
from django.db.models import Sum

from ledger.models import LedgerEntry
from pacts.models import OutcomeProposal


def pact_stats() -> list[dict[str, Any]]:
    """Win/loss/draw per member (overall and per kind), plus money won and lost on pacts.

    Void claims count for nobody. Members with no settled pacts are left out; the list is
    ordered by wins, then fewest losses."""
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

    users = get_user_model().objects.members().in_bulk(set(record) | set(won) | set(lost))
    rows = []
    for user_id, user in users.items():
        by_kind = {kind: dict(counts) for kind, counts in record.get(user_id, {}).items()}
        rows.append(
            {
                "user": user,
                "won": sum(c["won"] for c in by_kind.values()),
                "lost": sum(c["lost"] for c in by_kind.values()),
                "draw": sum(c["draw"] for c in by_kind.values()),
                "money_won": won.get(user_id, 0),
                "money_lost": lost.get(user_id, 0),
                "by_kind": by_kind,
            }
        )
    rows.sort(key=lambda r: (-r["won"], r["lost"], r["user"].username))
    return rows
