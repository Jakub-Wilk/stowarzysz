//! Users, activation links, refresh tokens, push subscriptions and cached exchange rates.

use crate::db::Uow;
use crate::media::Purpose;

use super::Problems;
use super::legacy::Legacy;
use super::media::MediaImport;

/// Django marks an account without a usable password with a leading `!`.
fn password_hash(raw: &str) -> Option<&str> {
    (!raw.is_empty() && !raw.starts_with('!')).then_some(raw)
}

fn hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

pub async fn import(
    uow: &mut Uow,
    data: &Legacy,
    media: &mut MediaImport,
    problems: &mut Problems,
) -> anyhow::Result<()> {
    for user in &data.users {
        let hash = password_hash(&user.password);
        if hash.is_some_and(|h| !h.starts_with("pbkdf2_sha256$")) {
            problems.push(format!(
                "user {} has a password hash this backend cannot verify",
                user.id
            ));
        }
        let avatar = if user.avatar.is_empty() {
            None
        } else {
            media
                .adopt(
                    format!("avatar of user {}", user.id),
                    &user.avatar,
                    Purpose::Avatar,
                )
                .await?
                .map(|image| image.key)
        };
        sqlx::query!(
            "insert into users (id, username, password_hash, is_active, is_superuser, avatar_key,
                                birthday, birthday_reminded_on, created_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            user.id,
            user.username,
            hash,
            user.is_active,
            user.is_superuser,
            avatar,
            user.birthday,
            user.birthday_reminded_on,
            user.date_joined,
        )
        .execute(uow.conn())
        .await?;
    }
    for token in &data.activation_tokens {
        let Some(hash) = hex(&token.token_hash) else {
            problems.push(format!(
                "activation token {} has a malformed hash",
                token.id
            ));
            continue;
        };
        sqlx::query!(
            "insert into activation_tokens (id, user_id, token_hash, created_by, created_at, expires_at, used_at)
             values ($1, $2, $3, $4, $5, $6, $7)",
            token.id,
            token.user_id,
            hash,
            token.created_by_id,
            token.created_at,
            token.expires_at,
            token.used_at,
        )
        .execute(uow.conn())
        .await?;
    }
    for token in &data.refresh_tokens {
        sqlx::query!(
            "insert into legacy_refresh_tokens (jti, user_id, expires_at) values ($1, $2, $3)",
            token.jti,
            token.user_id,
            token.expires_at,
        )
        .execute(uow.conn())
        .await?;
    }
    for sub in &data.push_subscriptions {
        sqlx::query!(
            "insert into push_subscriptions (id, user_id, endpoint, p256dh, auth, user_agent, created_at)
             values ($1, $2, $3, $4, $5, $6, $7)",
            sub.id,
            sub.user_id,
            sub.endpoint,
            sub.p256dh,
            sub.auth,
            sub.user_agent,
            sub.created_at,
        )
        .execute(uow.conn())
        .await?;
    }
    for rate in &data.rates {
        sqlx::query!(
            "insert into exchange_rates (currency, requested_on, rate, rate_date) values ($1, $2, $3, $4)",
            rate.currency,
            rate.requested_on,
            rate.rate,
            rate.rate_date,
        )
        .execute(uow.conn())
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unusable_passwords_become_not_activated() {
        assert_eq!(password_hash("!abc"), None);
        assert_eq!(password_hash(""), None);
        assert_eq!(
            password_hash("pbkdf2_sha256$1$s$h"),
            Some("pbkdf2_sha256$1$s$h")
        );
    }

    #[test]
    fn token_hashes_are_hex_digests() {
        assert_eq!(hex("00ff10"), Some(vec![0, 255, 16]));
        assert_eq!(hex("0g"), None);
        assert_eq!(hex("abc"), None);
    }
}
