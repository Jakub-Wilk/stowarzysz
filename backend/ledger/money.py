"""The ledger's arithmetic. Pure Python: no Django, no database, no requests.

Amounts are integers in a currency's minor units (grosze for PLN, whole yen for JPY), or, for
goods, a quantity of an item. Every function here sums exactly: nothing is ever lost to rounding.
"""

from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from decimal import ROUND_HALF_UP, Decimal
from heapq import heapify, heappop, heappush
from math import lcm

BASE_CURRENCY = "PLN"  # what balances are kept in; a constant, since changing it rewrites history

# ISO 4217 minor-unit exponents of the currencies the ECB publishes reference rates for; the
# common ones first (this is the order the currency picker shows).
CURRENCIES: dict[str, int] = {
    "PLN": 2,
    "EUR": 2,
    "USD": 2,
    "GBP": 2,
    "CHF": 2,
    "CZK": 2,
    "AUD": 2,
    "BRL": 2,
    "CAD": 2,
    "CNY": 2,
    "DKK": 2,
    "HKD": 2,
    "HUF": 2,
    "IDR": 2,
    "ILS": 2,
    "INR": 2,
    "ISK": 0,
    "JPY": 0,
    "KRW": 0,
    "MXN": 2,
    "MYR": 2,
    "NOK": 2,
    "NZD": 2,
    "PHP": 2,
    "RON": 2,
    "SEK": 2,
    "SGD": 2,
    "THB": 2,
    "TRY": 2,
    "ZAR": 2,
}


@dataclass(frozen=True)
class Debt:
    """The one fact the whole ledger is made of: `debtor` owes `creditor` `amount` minor units of
    the base currency or, with `item`, that many of the item ("piwo")."""

    debtor_id: int
    creditor_id: int
    amount: int
    item: str = ""


def allocate(total: int, weights: list[int]) -> list[int]:
    """Split `total` in proportion to `weights` in whole units, summing exactly to `total`
    (largest remainder; ties go to the earlier entry, so callers order their inputs)."""
    weight_sum = sum(weights)
    if total == 0 or weight_sum == 0:
        return [0] * len(weights)
    shares = [total * w // weight_sum for w in weights]
    leftover = total - sum(shares)
    by_remainder = sorted(
        range(len(weights)), key=lambda i: (-(total * weights[i] % weight_sum), i)
    )
    for i in by_remainder[:leftover]:
        shares[i] += 1
    return shares


def split_items(items: Sequence[tuple[int, Mapping[int, int]]]) -> dict[int, int]:
    """Divide each item (`amount`, `{user id: weight}`) among its people and total what each owes.

    Every item sums exactly, and its odd grosze go to whoever is furthest behind their exact share
    so far on the whole receipt (ties: lower user id), so the rounding evens out from item to item
    (four people sharing 108 zł of lines each owe 27 zł) instead of piling up on anyone. A single
    item is plain largest-remainder `allocate`. Errors are tracked in whole units of 1/scale grosz,
    so no floating point is involved."""
    scale = lcm(*(sum(weights.values()) for _, weights in items if weights), 1)
    behind: dict[int, int] = {}  # assigned minus exact share, times scale
    owed: dict[int, int] = {}
    for amount, weights in items:
        total_weight = sum(weights.values())
        if total_weight == 0:
            continue
        unit = scale // total_weight
        shares = {u: amount * w // total_weight for u, w in sorted(weights.items())}
        leftover = amount - sum(shares.values())
        # exact share minus what the floor gave; those with nothing to round are left out
        error = {
            u: behind.get(u, 0) + (shares[u] * total_weight - amount * w) * unit
            for u, w in sorted(weights.items())
            if amount * w % total_weight
        }
        for u in sorted(error, key=lambda u: (error[u], u))[:leftover]:
            shares[u] += 1
            error[u] += scale
        for u, share in shares.items():
            owed[u] = owed.get(u, 0) + share
            behind[u] = error.get(u, behind.get(u, 0))
    return owed


def settle_up(nets: Mapping[int, int], item: str = "") -> list[Debt]:
    """Transfers that bring every net to zero: the biggest debtor pays the biggest creditor, again
    and again. Each step clears at least one person, so n people need at most n-1 transfers, and
    ties go to the lower user id, so the same nets always give the same answer.

    `nets` maps user id to net (positive: is owed) and must sum to zero."""
    if sum(nets.values()) != 0:
        raise ValueError("nets must sum to zero")
    debtors = [(net, uid) for uid, net in nets.items() if net < 0]  # most negative first
    creditors = [(-net, uid) for uid, net in nets.items() if net > 0]  # largest credit first
    heapify(debtors)
    heapify(creditors)
    debts = []
    while debtors:
        owes, debtor = heappop(debtors)
        is_owed, creditor = heappop(creditors)
        amount = min(-owes, -is_owed)
        debts.append(Debt(debtor, creditor, amount, item))
        if -owes > amount:
            heappush(debtors, (owes + amount, debtor))
        if -is_owed > amount:
            heappush(creditors, (is_owed + amount, creditor))
    return debts


def to_base(amount: int, rate: Decimal, exponent: int) -> int:
    """`amount` minor units of a currency with `exponent` digits, in base-currency minor units
    at `rate` (base per unit), rounded half up."""
    scale = Decimal(10) ** (CURRENCIES[BASE_CURRENCY] - exponent)
    return int((amount * rate * scale).quantize(Decimal(1), ROUND_HALF_UP))


def _spread(total: int, parts: Mapping[int, int]) -> dict[int, int]:
    """`total` divided among `parts`' users in proportion to their amounts, in user id order."""
    ids = sorted(parts)
    return dict(zip(ids, allocate(total, [parts[i] for i in ids]), strict=True))


@dataclass(frozen=True)
class Breakdown:
    """Who paid and who owes what for one expense, in one currency, as {user id: amount}. Both
    sides sum to the same total; someone can be on both (they paid and ate)."""

    paid: dict[int, int]
    owed: dict[int, int]

    @property
    def total(self) -> int:
        return sum(self.paid.values())

    def nets(self) -> dict[int, int]:
        """Paid minus owed, per person (positive: the others owe them)."""
        people = self.paid.keys() | self.owed.keys()
        return {u: self.paid.get(u, 0) - self.owed.get(u, 0) for u in sorted(people)}

    def to_base(self, rate: Decimal, exponent: int) -> Breakdown:
        """The same breakdown in the base currency. The total is converted once (half up) and
        then spread over each side, so the result still balances to the grosz."""
        base_total = to_base(self.total, rate, exponent)
        return Breakdown(_spread(base_total, self.paid), _spread(base_total, self.owed))


def format_amount(amount: int, currency: str = BASE_CURRENCY, item: str = "") -> str:
    """For people (push texts): 750 -> '7,50 zł', 1000 -> '10 zł', 5 EUR -> '0,05 EUR',
    1200 JPY -> '1200 JPY'. Goods read as 'piwo' or '3 x piwo'."""
    if item:
        return item if amount == 1 else f"{amount} x {item}"
    exponent = CURRENCIES.get(currency, 2)
    major, minor = divmod(amount, 10**exponent)
    text = f"{major}" if minor == 0 else f"{major},{minor:0{exponent}d}"
    return f"{text} zł" if currency == BASE_CURRENCY else f"{text} {currency}"
