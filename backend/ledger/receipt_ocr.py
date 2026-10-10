"""Receipt OCR through Gemini structured output.

Every field is optional: a receipt can be cut off, creased or partly covered, and a field the
model cannot read must come back as null rather than a guess. Amounts are decimal numbers exactly
as printed (``12.99``), not minor units; converting them is the caller's job.
"""

import datetime as dt
import time
from collections.abc import Callable

from django.conf import settings
from google import genai
from google.genai import errors, types
from pydantic import BaseModel, Field

MAX_RETRIES = 2  # extra tries after the first one, on a 503 only
RETRY_DELAY = 1.0  # seconds between tries

PROMPT = (
    "Read this receipt and extract everything printed on it. Return null for anything that is not "
    "on the receipt or cannot be read; never guess or compute values that are not printed. Copy "
    "text as printed (diacritics included). Amounts are plain decimal numbers in the "
    "receipt's currency, e.g. 12.99, with discounts as positive numbers. Keep line items in the "
    "order printed."
)


class Merchant(BaseModel):
    name: str | None = Field(None, description="Store or brand name, as printed.")
    legal_name: str | None = Field(None, description="Registered company name, if different.")
    tax_id: str | None = Field(None, description="Tax ID (e.g. Polish NIP), as printed.")
    address: str | None = Field(None, description="Full address on one line.")
    phone: str | None = None
    website: str | None = None


class LineItem(BaseModel):
    name: str | None = Field(None, description="Product name as printed.")
    quantity: float | None = Field(None, description="Quantity or weight; 1 if none is shown.")
    unit: str | None = Field(None, description="Unit of the quantity (szt., kg, l, ...).")
    unit_price: float | None = Field(None, description="Price of one unit.")
    total: float | None = Field(None, description="Line total after line discounts.")
    discount: float | None = Field(None, description="Discount applied to this line, positive.")
    tax_code: str | None = Field(None, description="Tax letter or rate on the line (A, 23%).")
    sku: str | None = Field(None, description="Barcode, SKU or product code.")


class TaxLine(BaseModel):
    code: str | None = Field(None, description="Tax letter, e.g. A, B, C.")
    rate_percent: float | None = None
    net: float | None = Field(None, description="Net amount (sprzedaż netto).")
    tax: float | None = Field(None, description="Tax amount (kwota PTU / VAT).")
    gross: float | None = None


class Payment(BaseModel):
    method: str | None = Field(None, description="cash, card, blik, voucher, ... as printed.")
    amount: float | None = None
    card_last4: str | None = None
    card_scheme: str | None = Field(None, description="Visa, Mastercard, ...")


class Receipt(BaseModel):
    is_receipt: bool | None = Field(None, description="False if the image is not a receipt.")
    merchant: Merchant | None = None
    receipt_number: str | None = None
    fiscal_number: str | None = Field(None, description="Fiscal / KSeF / transaction number.")
    date: dt.date | None = Field(None, description="Purchase date.")
    time: dt.time | None = Field(None, description="Purchase time.")
    cashier: str | None = None
    till: str | None = Field(None, description="Till, register or terminal identifier.")
    currency: str | None = Field(
        None,
        description="ISO 4217 code, e.g. PLN; derive it from a printed symbol (zł, €, £) or code.",
    )
    items: list[LineItem] | None = None
    subtotal: float | None = Field(None, description="Sum before receipt-level discounts, tips.")
    discount_total: float | None = Field(None, description="Receipt-level discounts, positive.")
    service_charge: float | None = None
    tip: float | None = None
    rounding: float | None = None
    tax_lines: list[TaxLine] | None = None
    tax_total: float | None = None
    total: float | None = Field(None, description="Amount due (SUMA / DO ZAPŁATY).")
    payments: list[Payment] | None = None
    change: float | None = Field(None, description="Change given back (reszta).")
    loyalty_info: str | None = Field(None, description="Loyalty card, points or coupons printed.")
    notes: str | None = Field(None, description="Other relevant printed text.")
    language: str | None = Field(None, description="ISO 639-1 code of the receipt's language.")
    unreadable_parts: str | None = Field(
        None, description="Short note on parts that are cut off, blurred or obscured."
    )


def _generate(client: genai.Client, image: bytes, mime_type: str) -> Receipt:
    response = client.models.generate_content(
        model=settings.GEMINI_MODEL,
        contents=[types.Part.from_bytes(data=image, mime_type=mime_type), PROMPT],
        config=types.GenerateContentConfig(
            response_mime_type="application/json",
            response_schema=Receipt,
            temperature=0,
        ),
    )
    if isinstance(response.parsed, Receipt):
        return response.parsed
    return Receipt.model_validate_json(response.text or "{}")


def ocr_receipt(
    image: bytes,
    mime_type: str = "image/jpeg",
    on_request_sent: Callable[[int], None] | None = None,
) -> Receipt:
    """Send one receipt image to Gemini and return the parsed result (blocking network call).

    A 503 (model overloaded) is retried up to `MAX_RETRIES` times after `RETRY_DELAY` seconds;
    anything else propagates. `on_request_sent(attempt)` is called with the 1-based attempt number
    right before each request goes out.
    """
    if not settings.GEMINI_API_KEY:
        raise RuntimeError("GEMINI_API_KEY is not set")
    client = genai.Client(api_key=settings.GEMINI_API_KEY)
    for attempt in range(1, MAX_RETRIES + 2):
        if on_request_sent:
            on_request_sent(attempt)
        try:
            return _generate(client, image, mime_type)
        except errors.ServerError as exc:
            if exc.code != 503 or attempt > MAX_RETRIES:
                raise
        time.sleep(RETRY_DELAY)
    raise AssertionError("unreachable")  # pragma: no cover
