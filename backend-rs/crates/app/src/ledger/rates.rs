//! Exchange rates for the day something was paid: ECB reference rates from Frankfurter.
//!
//! A rate is fetched once per currency and day and kept in `exchange_rates` for good, since a
//! past day's rate never changes. The exception is today before the ECB publishes (~16:00 CET):
//! Frankfurter then answers with the last published rate, which is used but not stored.
//! Concurrent requests for the same rate share one fetch. Fetching never happens inside a
//! transaction; the expense form warms the cache through `GET /api/ledger/rates`.

use std::collections::HashMap;
use std::str::FromStr as _;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::NaiveDate;
use domain::DomainError;
use domain::ledger::money::{self, BASE_CURRENCY};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::{AppError, AppResult};

const TIMEOUT: Duration = Duration::from_secs(5);
/// Decimal places rates are kept with (as the Python backend stored them).
const PRECISION: u32 = 8;

/// One unit of a currency is worth `value` PLN, as published on `day`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rate {
    pub value: Decimal,
    pub day: NaiveDate,
}

/// The rate an entry in `currency` dated `date` uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct RateView {
    pub currency: String,
    /// PLN per unit, as a decimal string without trailing zeros (e.g. `4.2537`).
    #[schema(value_type = String)]
    pub rate: Decimal,
    /// The day the ECB published it (a weekend uses Friday's).
    pub rate_date: NaiveDate,
}

type Flight = Arc<tokio::sync::Mutex<()>>;

/// The rate source of this process.
pub struct Rates {
    client: reqwest::Client,
    base_url: String,
    flights: Mutex<HashMap<(String, NaiveDate), Flight>>,
}

impl std::fmt::Debug for Rates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rates")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct Answer {
    date: NaiveDate,
    rates: HashMap<String, serde_json::Number>,
}

fn unavailable() -> AppError {
    AppError::unavailable(
        "rate_unavailable",
        "Nie udało się pobrać kursu waluty. Spróbuj ponownie za chwilę.",
    )
}

impl Rates {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(TIMEOUT)
                .user_agent("stowarzysz")
                .build()
                .unwrap_or_default(),
            base_url: base_url.to_owned(),
            flights: Mutex::new(HashMap::new()),
        }
    }

    /// The rate of `currency` on `day` (`today` is the Warsaw date, which decides whether a
    /// fresh answer may be kept).
    pub async fn rate(
        &self,
        db: &PgPool,
        currency: &str,
        day: NaiveDate,
        today: NaiveDate,
    ) -> AppResult<Rate> {
        if currency == BASE_CURRENCY {
            return Ok(Rate {
                value: Decimal::ONE,
                day,
            });
        }
        if money::exponent(currency).is_none() {
            return Err(
                DomainError::new("unsupported_currency", "Nieobsługiwana waluta.")
                    .on("currency")
                    .into(),
            );
        }
        if let Some(rate) = cached(db, currency, day).await? {
            return Ok(rate);
        }
        let key = (currency.to_owned(), day);
        let flight = Arc::clone(self.lock_flights().entry(key.clone()).or_default());
        let result = {
            let _turn = flight.lock().await;
            // Whoever held the turn before may have stored it meanwhile.
            match cached(db, currency, day).await {
                Ok(Some(rate)) => Ok(rate),
                Ok(None) => self.fetch_and_keep(db, currency, day, today).await,
                Err(error) => Err(error),
            }
        };
        drop(flight);
        let mut flights = self.lock_flights();
        if flights.get(&key).is_some_and(|f| Arc::strong_count(f) == 1) {
            flights.remove(&key);
        }
        result
    }

    fn lock_flights(&self) -> std::sync::MutexGuard<'_, HashMap<(String, NaiveDate), Flight>> {
        self.flights
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    async fn fetch_and_keep(
        &self,
        db: &PgPool,
        currency: &str,
        day: NaiveDate,
        today: NaiveDate,
    ) -> AppResult<Rate> {
        let rate = self.fetch(currency, day).await?;
        if rate.day == day || day < today {
            sqlx::query!(
                "insert into exchange_rates (currency, requested_on, rate, rate_date)
                 values ($1, $2, $3, $4) on conflict do nothing",
                currency,
                day,
                rate.value,
                rate.day,
            )
            .execute(db)
            .await?;
        }
        Ok(rate)
    }

    /// Ask Frankfurter; it answers for the last working day up to `day`.
    async fn fetch(&self, currency: &str, day: NaiveDate) -> AppResult<Rate> {
        let url = format!(
            "{}/{day}?base={currency}&symbols={BASE_CURRENCY}",
            self.base_url
        );
        let response = self.client.get(&url).send().await.map_err(|error| {
            tracing::warn!(%error, currency, %day, "exchange rate request failed");
            unavailable()
        })?;
        if !response.status().is_success() {
            tracing::warn!(status = %response.status(), currency, %day, "exchange rate refused");
            return Err(unavailable());
        }
        let answer: Answer = response.json().await.map_err(|error| {
            tracing::warn!(%error, currency, %day, "unexpected exchange rate answer");
            unavailable()
        })?;
        // `Number` keeps the shortest decimal that round-trips, i.e. the digits as published.
        let value = answer
            .rates
            .get(BASE_CURRENCY)
            .and_then(|number| Decimal::from_str(&number.to_string()).ok())
            .filter(|value| *value > Decimal::ZERO)
            .ok_or_else(unavailable)?;
        Ok(Rate {
            value: value.round_dp_with_strategy(PRECISION, RoundingStrategy::MidpointNearestEven),
            day: answer.date,
        })
    }
}

async fn cached(db: &PgPool, currency: &str, day: NaiveDate) -> AppResult<Option<Rate>> {
    Ok(sqlx::query!(
        "select rate, rate_date from exchange_rates where currency = $1 and requested_on = $2",
        currency,
        day,
    )
    .fetch_optional(db)
    .await?
    .map(|row| Rate {
        value: row.rate,
        day: row.rate_date,
    }))
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[tokio::test]
    async fn fetch_reads_frankfurter_and_keeps_the_published_digits() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/2026-03-14"))
            .and(query_param("base", "EUR"))
            .and(query_param("symbols", "PLN"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"{"amount":1.0,"base":"EUR","date":"2026-03-13","rates":{"PLN":4.2537}}"#,
                "application/json",
            ))
            .mount(&server)
            .await;
        let rates = Rates::new(&server.uri());
        let day = NaiveDate::from_ymd_opt(2026, 3, 14).unwrap();
        let rate = rates.fetch("EUR", day).await.unwrap();
        assert_eq!(rate.value.normalize().to_string(), "4.2537");
        assert_eq!(rate.day, NaiveDate::from_ymd_opt(2026, 3, 13).unwrap());
    }

    #[tokio::test]
    async fn failures_become_a_polish_503() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;
        let day = NaiveDate::from_ymd_opt(2026, 3, 14).unwrap();
        for rates in [Rates::new(&server.uri()), Rates::new("http://127.0.0.1:9")] {
            let error = rates.fetch("EUR", day).await.unwrap_err();
            assert!(matches!(
                error,
                AppError::Unavailable {
                    code: "rate_unavailable",
                    ..
                }
            ));
        }
    }
}
