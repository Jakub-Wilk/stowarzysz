"""Generate golden vectors from the Django backend's pure functions.

The Rust port must reproduce these bit for bit: historic ledger breakdowns are recomputed from
stored `details`, so `split_items` & co. must give the same grosze as the Python did. Run from
`backend-rs/` (needs the Python backend, which stays in the repo until cutover):

    uv run --project ../backend python fixtures/differential/generate.py

The RNG is seeded, so regenerating without code changes reproduces the files exactly. JSON has no
integer keys, so every {user id: value} map is written as sorted [id, value] pairs.
"""

import json
import os
import random
import sys
from decimal import Decimal
from pathlib import Path

HERE = Path(__file__).resolve().parent
BACKEND = HERE.parents[2] / "backend"
sys.path.insert(0, str(BACKEND))
os.environ.setdefault("DJANGO_SETTINGS_MODULE", "config.settings")
os.environ.setdefault("DEBUG", "True")  # dev defaults for the secrets; no database is touched

import django  # noqa: E402

django.setup()

from ledger.money import (  # noqa: E402
    CURRENCIES,
    Breakdown,
    allocate,
    format_amount,
    settle_up,
    split_items,
    to_base,
)
from pacts.kinds import BetKind, GroupBetKind, Party  # noqa: E402
from secretsanta.services import _per_tier_shifts  # noqa: E402
from voting.tally import count_picks, schulze, top_ids  # noqa: E402

rng = random.Random(20261112)


def pairs(mapping):
    return [[k, v] for k, v in sorted(mapping.items())]


def write(name, cases, about):
    path = HERE / f"{name}.json"
    compact = {"ensure_ascii": False, "separators": (",", ":")}
    lines = ",\n".join(json.dumps(case, **compact) for case in cases)
    text = f'{{"about":{json.dumps(about, **compact)},"cases":[\n{lines}\n]}}\n'
    path.write_text(text, encoding="utf-8", newline="\n")
    print(f"{path.name}: {len(cases)} cases, {len(text.encode()) // 1024} KiB")


# --- money.allocate -------------------------------------------------------------------------
def allocate_cases():
    fixed = [
        (0, [1, 2, 3]),
        (10, [0, 0]),
        (10, [1]),
        (100, [1, 1, 1]),  # remainder ties go to the earlier index
        (1, [1, 1, 1]),
        (5, [3, 3]),
        (7, [2, 1, 1]),
        (1001, [3334, 3333, 3333]),
        (14182 + 14178 + 14177, [3334, 3333, 3333]),
        (10**15, [1, 2, 3, 5, 7]),
        (999_999_937, [999_983, 999_979, 1]),
    ]
    cases = [{"total": t, "weights": w, "output": allocate(t, w)} for t, w in fixed]
    for _ in range(300):
        n = rng.randint(1, 12)
        weights = [rng.choice([0, 1, rng.randint(1, 50), rng.randint(1, 100_000)]) for _ in range(n)]
        total = rng.choice([rng.randint(0, 100), rng.randint(0, 10**6), rng.randint(0, 10**12)])
        cases.append({"total": total, "weights": weights, "output": allocate(total, weights)})
    return cases


# --- money.split_items ----------------------------------------------------------------------
def receipt(n_items, people, exact_share=0.2):
    items = []
    for _ in range(n_items):
        amount = rng.randint(1, 50_000)
        who = rng.sample(people, rng.randint(1, len(people)))
        mode = rng.random()
        if mode < exact_share:  # exact amounts: the weights are the amounts themselves
            cuts = sorted(rng.sample(range(1, amount), min(len(who) - 1, amount - 1))) if amount > 1 else []
            parts = [b - a for a, b in zip([0, *cuts], [*cuts, amount], strict=False)]
            shares = {u: p for u, p in zip(who, parts, strict=False) if p > 0}
        elif mode < 0.5:  # shares
            shares = {u: rng.randint(1, 5) for u in who}
        else:  # equal
            shares = dict.fromkeys(who, 1)
        items.append((amount, shares))
    return items


def split_cases():
    pinned = [(a, dict.fromkeys([1, 2, 3, 4], 1)) for a in (1950, 1400, 3700, 1950, 1800)]
    receipts = [pinned, [(100, {1: 1, 2: 1, 3: 1})], [(1, {5: 1, 2: 1})], [(10, {9: 0})]]
    for _ in range(250):
        people = rng.sample(range(1, 40), rng.randint(1, 8))
        receipts.append(receipt(rng.randint(1, 12), people))
    # Many exact-split lines: the lcm of their totals is astronomical (bigint territory).
    receipts.append(receipt(200, list(range(1, 9)), exact_share=1.0))
    return [
        {
            "items": [[amount, pairs(shares)] for amount, shares in items],
            "output": pairs(split_items(items)),
        }
        for items in receipts
    ]


# --- money.settle_up ------------------------------------------------------------------------
def settle_cases():
    def balanced(n):
        nets = {u: rng.randint(-50_000, 50_000) for u in rng.sample(range(1, 60), n)}
        fix = rng.choice(list(nets))
        nets[fix] -= sum(nets.values())
        return nets

    nets_list = [{}, {1: 0, 2: 0}, {1: 100, 2: -100}, {1: 50, 2: 50, 3: -50, 4: -50}]
    nets_list += [balanced(rng.randint(2, 14)) for _ in range(250)]
    return [
        {
            "nets": pairs(nets),
            "output": [[d.debtor_id, d.creditor_id, d.amount] for d in settle_up(nets)],
        }
        for nets in nets_list
    ]


# --- money.to_base / Breakdown.to_base ------------------------------------------------------
def rate():
    return Decimal(rng.randint(1, 10**10)) / Decimal(10**8)


def to_base_cases():
    fixed = [(1, Decimal("0.5"), 2), (3, Decimal("0.5"), 2), (1, Decimal("0.005"), 0), (10000, Decimal("4.2537"), 2)]
    cases = [(a, r, e) for a, r, e in fixed]
    for _ in range(300):
        exponent = rng.choice([0, 2, 2, 2])
        cases.append((rng.randint(0, 10**9), rate(), exponent))
    return [{"amount": a, "rate": str(r), "exponent": e, "output": to_base(a, r, e)} for a, r, e in cases]


def breakdown_cases():
    cases = [(Breakdown({1: 10000}, {1: 3334, 2: 3333, 3: 3333}), Decimal("4.2537"), 2)]
    for _ in range(200):
        people = rng.sample(range(1, 30), rng.randint(1, 6))
        owed = split_items(receipt(rng.randint(1, 5), people))
        total = sum(owed.values())
        payers = rng.sample(people, rng.randint(1, len(people)))
        paid = dict(zip(sorted(payers), allocate(total, [rng.randint(1, 9) for _ in payers]), strict=True))
        paid = {u: v for u, v in paid.items() if v}
        cases.append((Breakdown(paid, owed), rate(), rng.choice([0, 2])))
    out = []
    for breakdown, r, e in cases:
        base = breakdown.to_base(r, e)
        out.append(
            {
                "paid": pairs(breakdown.paid),
                "owed": pairs(breakdown.owed),
                "rate": str(r),
                "exponent": e,
                "output": {"paid": pairs(base.paid), "owed": pairs(base.owed)},
            }
        )
    return out


# --- format_amount --------------------------------------------------------------------------
def format_cases():
    fixed = [(750, "PLN", ""), (1000, "PLN", ""), (5, "EUR", ""), (1200, "JPY", ""), (1, "PLN", "piwo"), (3, "PLN", "piwo"), (0, "PLN", ""), (1234567, "USD", "")]
    for _ in range(100):
        fixed.append((rng.randint(0, 10**7), rng.choice(list(CURRENCIES)), ""))
    return [{"amount": a, "currency": c, "item": i, "output": format_amount(a, c, i)} for a, c, i in fixed]


# --- voting.tally ---------------------------------------------------------------------------
def option_ids(n):
    return [f"o{i}" for i in range(1, n + 1)]


def schulze_cases():
    cases = [
        (option_ids(3), []),
        (option_ids(3), [["o1", "o2", "o3"]]),
        (option_ids(3), [["o1", "o2", "o3"], ["o2", "o3", "o1"], ["o3", "o1", "o2"]]),  # cycle
        (option_ids(2), [["o1"], ["o2"]]),  # tie
        (option_ids(4), [["o2"], ["o2", "o1"], ["o3"]]),  # partial rankings
    ]
    for _ in range(250):
        ids = option_ids(rng.randint(2, 12))
        ballots = []
        for _ in range(rng.randint(0, 15)):
            ranking = rng.sample(ids, rng.randint(1, len(ids)))
            ballots.append(ranking)
        cases.append((ids, ballots))
    out = []
    for ids, ballots in cases:
        result = schulze(ids, ballots)
        out.append(
            {
                "options": ids,
                "rankings": ballots,
                "output": {
                    "pairwise": result.pairwise,
                    "wins": result.wins,
                    "winners": result.winners,
                    "condorcet_winner": result.condorcet_winner,
                },
            }
        )
    return out


def picks_cases():
    cases = [(option_ids(3), []), (option_ids(2), [["o1"], ["o2"]])]
    for _ in range(150):
        ids = option_ids(rng.randint(2, 20))
        picks = [rng.sample(ids, rng.randint(1, min(3, len(ids)))) for _ in range(rng.randint(0, 20))]
        cases.append((ids, picks))
    out = []
    for ids, picks in cases:
        counts = count_picks(ids, picks)
        out.append({"options": ids, "picks": picks, "output": {"counts": counts, "top": top_ids(ids, counts)}})
    return out


# --- pacts settle ---------------------------------------------------------------------------
def settlement_json(settlement):
    return {
        "debts": [[d.debtor_id, d.creditor_id, d.amount, d.item] for d in settlement.debts],
        "verdicts": pairs(settlement.verdicts),
    }


def party_json(p):
    return {"user_id": p.user_id, "is_host": p.is_host, "side": p.side, "stake_amount": p.stake_amount, "stake_note": p.stake_note}


def pact_cases():
    out = []
    bet = BetKind()
    for winner in ("host", "opponent", "draw"):
        for stake, note in ((2500, ""), (None, "kolacja"), (None, ""), (None, "x" * 80)):
            parties = [Party(1, True, "", None), Party(2, False, "", stake, note)]
            result = {"winner": winner}
            out.append({"kind": "bet", "parties": [party_json(p) for p in parties], "result": result, "config": {}, "output": settlement_json(bet.settle(parties, result, {}))})
    group = GroupBetKind()
    for _ in range(250):
        sides = rng.sample(["tak", "nie", "może", "remis?", "A", "B"], rng.randint(2, 4))
        parties = []
        for i, user in enumerate(rng.sample(range(1, 50), rng.randint(2, 9))):
            stake = rng.choice([None, None, rng.randint(1, 100) * 100, rng.randint(1, 99_999)])
            note = rng.choice(["", "", "piwo", "kolacja dla wszystkich"]) if stake is None else rng.choice(["", "", "piwo"])
            parties.append(Party(user, i == 0, rng.choice(sides), stake, note))
        winner = rng.choice([*sides, "draw"])
        config = {"sides": sides}
        result = {"winner": winner}
        out.append({"kind": "group_bet", "parties": [party_json(p) for p in parties], "result": result, "config": config, "output": settlement_json(group.settle(parties, result, config))})
    return out


# --- secret santa ---------------------------------------------------------------------------
def shift_cases():
    return [{"size": n, "output": _per_tier_shifts(n)} for n in range(1, 41)]


if __name__ == "__main__":
    write("allocate", allocate_cases(), "ledger.money.allocate(total, weights)")
    write("split_items", split_cases(), "ledger.money.split_items(items): items are [amount, [[user, weight]]]")
    write("settle_up", settle_cases(), "ledger.money.settle_up(nets): output is [debtor, creditor, amount]")
    write("to_base", to_base_cases(), "ledger.money.to_base(amount, rate, exponent)")
    write("breakdown_to_base", breakdown_cases(), "ledger.money.Breakdown(paid, owed).to_base(rate, exponent)")
    write("format_amount", format_cases(), "ledger.money.format_amount(amount, currency, item)")
    write("schulze", schulze_cases(), "voting.tally.schulze(options, rankings)")
    write("picks", picks_cases(), "voting.tally.count_picks + top_ids")
    write("pact_settle", pact_cases(), "pacts.kinds.<Kind>().settle(parties, result, config): debts are [debtor, creditor, amount, item]")
    write("santa_shifts", shift_cases(), "secretsanta.services._per_tier_shifts(size)")
