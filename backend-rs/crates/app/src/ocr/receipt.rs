//! What a receipt reads as. Every field is optional: a receipt can be cut off, creased or partly
//! covered, and a field the model cannot read must come back as null rather than a guess.
//! Amounts are decimal numbers exactly as printed (`12.99`), not minor units; converting them is
//! the client's job.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;

/// The instruction sent with every image.
pub const PROMPT: &str = "Read this receipt and extract everything printed on it. Return null for \
    anything that is not on the receipt or cannot be read; never guess or compute values that are \
    not printed. Copy text as printed (diacritics included). Amounts are plain decimal numbers in \
    the receipt's currency, e.g. 12.99, with discounts as positive numbers. Keep line items in \
    the order printed.";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct Merchant {
    pub name: Option<String>,
    pub legal_name: Option<String>,
    pub tax_id: Option<String>,
    pub address: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct LineItem {
    pub name: Option<String>,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub unit_price: Option<f64>,
    /// After line discounts.
    pub total: Option<f64>,
    pub discount: Option<f64>,
    pub tax_code: Option<String>,
    pub sku: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct TaxLine {
    pub code: Option<String>,
    pub rate_percent: Option<f64>,
    pub net: Option<f64>,
    pub tax: Option<f64>,
    pub gross: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct Payment {
    pub method: Option<String>,
    pub amount: Option<f64>,
    pub card_last4: Option<String>,
    pub card_scheme: Option<String>,
}

/// A receipt as read by the model.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct Receipt {
    /// False if the image is not a receipt.
    pub is_receipt: Option<bool>,
    pub merchant: Option<Merchant>,
    pub receipt_number: Option<String>,
    pub fiscal_number: Option<String>,
    /// `YYYY-MM-DD`, as the model read it.
    pub date: Option<String>,
    /// `HH:MM[:SS]`.
    pub time: Option<String>,
    pub cashier: Option<String>,
    pub till: Option<String>,
    /// ISO 4217, e.g. `PLN`.
    pub currency: Option<String>,
    pub items: Option<Vec<LineItem>>,
    pub subtotal: Option<f64>,
    pub discount_total: Option<f64>,
    pub service_charge: Option<f64>,
    pub tip: Option<f64>,
    pub rounding: Option<f64>,
    pub tax_lines: Option<Vec<TaxLine>>,
    pub tax_total: Option<f64>,
    /// Amount due (SUMA / DO ZAPŁATY).
    pub total: Option<f64>,
    pub payments: Option<Vec<Payment>>,
    /// Change given back (reszta).
    pub change: Option<f64>,
    pub loyalty_info: Option<String>,
    pub notes: Option<String>,
    /// ISO 639-1.
    pub language: Option<String>,
    pub unreadable_parts: Option<String>,
}

fn text(description: &str) -> Value {
    json!({"type": "STRING", "nullable": true, "description": description})
}

fn number(description: &str) -> Value {
    json!({"type": "NUMBER", "nullable": true, "description": description})
}

fn object(properties: &[(&str, Value)]) -> Value {
    let map: serde_json::Map<String, Value> = properties
        .iter()
        .map(|(name, schema)| ((*name).to_owned(), schema.clone()))
        .collect();
    let order: Vec<&str> = properties.iter().map(|(name, _)| *name).collect();
    json!({"type": "OBJECT", "nullable": true, "properties": map, "propertyOrdering": order})
}

fn list(items: &Value, description: &str) -> Value {
    json!({"type": "ARRAY", "nullable": true, "items": items, "description": description})
}

/// The `responseSchema` (Gemini's OpenAPI subset) matching [`Receipt`].
pub fn response_schema() -> Value {
    let merchant = object(&[
        ("name", text("Store or brand name, as printed.")),
        ("legal_name", text("Registered company name, if different.")),
        ("tax_id", text("Tax ID (e.g. Polish NIP), as printed.")),
        ("address", text("Full address on one line.")),
        ("phone", text("")),
        ("website", text("")),
    ]);
    let item = object(&[
        ("name", text("Product name as printed.")),
        (
            "quantity",
            number("Quantity or weight; 1 if none is shown."),
        ),
        ("unit", text("Unit of the quantity (szt., kg, l, ...).")),
        ("unit_price", number("Price of one unit.")),
        ("total", number("Line total after line discounts.")),
        (
            "discount",
            number("Discount applied to this line, positive."),
        ),
        ("tax_code", text("Tax letter or rate on the line (A, 23%).")),
        ("sku", text("Barcode, SKU or product code.")),
    ]);
    let tax = object(&[
        ("code", text("Tax letter, e.g. A, B, C.")),
        ("rate_percent", number("")),
        ("net", number("Net amount (sprzedaż netto).")),
        ("tax", number("Tax amount (kwota PTU / VAT).")),
        ("gross", number("")),
    ]);
    let payment = object(&[
        ("method", text("cash, card, blik, voucher, ... as printed.")),
        ("amount", number("")),
        ("card_last4", text("")),
        ("card_scheme", text("Visa, Mastercard, ...")),
    ]);
    object(&[
        (
            "is_receipt",
            json!({"type": "BOOLEAN", "nullable": true, "description": "False if the image is not a receipt."}),
        ),
        ("merchant", merchant),
        ("receipt_number", text("")),
        ("fiscal_number", text("Fiscal / KSeF / transaction number.")),
        ("date", text("Purchase date, YYYY-MM-DD.")),
        ("time", text("Purchase time, HH:MM or HH:MM:SS.")),
        ("cashier", text("")),
        ("till", text("Till, register or terminal identifier.")),
        (
            "currency",
            text("ISO 4217 code, e.g. PLN; derive it from a printed symbol (zł, €, £) or code."),
        ),
        ("items", list(&item, "Line items in the order printed.")),
        (
            "subtotal",
            number("Sum before receipt-level discounts, tips."),
        ),
        (
            "discount_total",
            number("Receipt-level discounts, positive."),
        ),
        ("service_charge", number("")),
        ("tip", number("")),
        ("rounding", number("")),
        ("tax_lines", list(&tax, "")),
        ("tax_total", number("")),
        ("total", number("Amount due (SUMA / DO ZAPŁATY).")),
        ("payments", list(&payment, "")),
        ("change", number("Change given back (reszta).")),
        (
            "loyalty_info",
            text("Loyalty card, points or coupons printed."),
        ),
        ("notes", text("Other relevant printed text.")),
        (
            "language",
            text("ISO 639-1 code of the receipt's language."),
        ),
        (
            "unreadable_parts",
            text("Short note on parts that are cut off, blurred or obscured."),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every field the model may return is one `Receipt` reads, and the other way round.
    #[test]
    fn the_schema_and_the_type_agree() {
        fn names(schema: &Value) -> Vec<String> {
            let mut names: Vec<String> = schema["properties"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();
            names.sort();
            names
        }
        fn fields(value: &Value) -> Vec<String> {
            let mut names: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
            names.sort();
            names
        }
        let schema = response_schema();
        assert_eq!(
            names(&schema),
            fields(&serde_json::to_value(Receipt::default()).unwrap())
        );
        let nested = [
            (
                "merchant",
                serde_json::to_value(Merchant::default()).unwrap(),
            ),
            ("items", serde_json::to_value(LineItem::default()).unwrap()),
            (
                "tax_lines",
                serde_json::to_value(TaxLine::default()).unwrap(),
            ),
            (
                "payments",
                serde_json::to_value(Payment::default()).unwrap(),
            ),
        ];
        for (field, value) in nested {
            let sub = &schema["properties"][field];
            let sub = if sub["type"] == "ARRAY" {
                &sub["items"]
            } else {
                sub
            };
            assert_eq!(names(sub), fields(&value), "{field}");
        }
        let read: Receipt =
            serde_json::from_str(r#"{"total": 12.5, "merchant": {"name": "Żabka"}, "extra": 1}"#)
                .unwrap();
        assert_eq!(read.total, Some(12.5));
        assert_eq!(read.merchant.unwrap().name.as_deref(), Some("Żabka"));
    }
}
