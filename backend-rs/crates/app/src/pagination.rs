//! Keyset pagination: an opaque cursor naming the last item of a page (its sort key and id), so
//! paging stays correct while rows are added and costs an index seek, not an offset scan.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use domain::DomainError;

/// The position after which the next page starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub at: DateTime<Utc>,
    pub id: i64,
}

impl Cursor {
    /// Ahead of every row: where the first page starts. Queries always compare against a real
    /// cursor (never `$1 is null or ...`), so the keyset seek stays an index range scan even when
    /// Postgres plans the prepared statement generically.
    pub fn start() -> Self {
        Self {
            at: DateTime::from_timestamp(253_402_300_799, 0).unwrap_or_default(), // 9999-12-31
            id: i64::MAX,
        }
    }

    pub fn encode(self) -> String {
        URL_SAFE_NO_PAD.encode(format!("{}:{}", self.at.timestamp_micros(), self.id))
    }

    /// Parse a cursor from a client; a malformed one is a 400 on `cursor`.
    pub fn decode(raw: &str) -> Result<Self, DomainError> {
        let invalid = || {
            DomainError::new("invalid_cursor", "Nieprawidłowy kursor stronicowania.").on("cursor")
        };
        let text = URL_SAFE_NO_PAD
            .decode(raw)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .ok_or_else(invalid)?;
        let (micros, id) = text.split_once(':').ok_or_else(invalid)?;
        let at = micros
            .parse()
            .ok()
            .and_then(DateTime::from_timestamp_micros)
            .ok_or_else(invalid)?;
        Ok(Self {
            at,
            id: id.parse().map_err(|_| invalid())?,
        })
    }
}

/// Split one extra fetched row off `rows` (fetch `limit + 1`): the page, and the cursor of the
/// next page if there is one.
pub fn page<T>(
    mut rows: Vec<T>,
    limit: usize,
    key: impl Fn(&T) -> Cursor,
) -> (Vec<T>, Option<String>) {
    if rows.len() <= limit {
        return (rows, None);
    }
    rows.truncate(limit);
    let next = rows.last().map(|last| key(last).encode());
    (rows, next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursors_round_trip_and_reject_garbage() {
        let cursor = Cursor {
            at: DateTime::from_timestamp_micros(1_798_000_000_123_456).unwrap(),
            id: 42,
        };
        assert_eq!(Cursor::decode(&cursor.encode()), Ok(cursor));
        for bad in ["", "!!", "bm9wZQ", "MTI6"] {
            assert_eq!(
                Cursor::decode(bad).unwrap_err().code,
                "invalid_cursor",
                "{bad}"
            );
        }
    }

    #[test]
    fn page_splits_off_the_extra_row() {
        let key = |n: &i64| Cursor {
            at: DateTime::from_timestamp(*n, 0).unwrap(),
            id: *n,
        };
        let (items, next) = page(vec![5, 4, 3], 2, key);
        assert_eq!(items, [5, 4]);
        assert_eq!(Cursor::decode(&next.unwrap()).unwrap().id, 4);
        assert_eq!(page(vec![5, 4], 2, key), (vec![5, 4], None));
    }
}
