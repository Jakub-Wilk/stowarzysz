//! What members see about themselves and each other.

use domain::accounts::birthday;

use super::views::{LoginUser, Me, Person};
use crate::clock::local_today;
use crate::{AppError, AppResult, Context};

pub async fn me(ctx: &Context, user_id: i64) -> AppResult<Me> {
    let row = sqlx::query!(
        "select id, username, is_superuser, avatar_key, birthday from users where id = $1",
        user_id,
    )
    .fetch_optional(&ctx.db)
    .await?
    .ok_or(AppError::NotFound)?;
    let now = ctx.clock.now();
    Ok(Me {
        id: row.id,
        username: row.username,
        is_superuser: row.is_superuser,
        avatar: row.avatar_key.map(|key| ctx.media.image(&key, now)),
        birthday: row.birthday,
    })
}

/// Members (active and activated): the people who can take part in things.
pub async fn people(ctx: &Context) -> AppResult<Vec<Person>> {
    let rows = sqlx::query!(
        "select id, username, avatar_key from users
         where is_active and password_hash is not null order by username",
    )
    .fetch_all(&ctx.db)
    .await?;
    let now = ctx.clock.now();
    Ok(rows
        .into_iter()
        .map(|r| Person {
            id: r.id,
            username: r.username,
            avatar: r.avatar_key.map(|k| ctx.media.image(&k, now)),
        })
        .collect())
}

/// The public login screen lists members by name and picture (a small, closed group).
pub async fn login_users(ctx: &Context) -> AppResult<Vec<LoginUser>> {
    Ok(people(ctx)
        .await?
        .into_iter()
        .map(|person| LoginUser {
            username: person.username,
            avatar: person.avatar,
        })
        .collect())
}

/// Members whose birthday is today (Warsaw time).
pub async fn birthdays_today(ctx: &Context) -> AppResult<Vec<Person>> {
    let days = month_days(&birthday::birthdays_on(local_today(ctx.clock.now())));
    let rows = sqlx::query!(
        "select id, username, avatar_key from users
         where is_active and password_hash is not null and to_char(birthday, 'MM-DD') = any($1)
         order by username",
        &days,
    )
    .fetch_all(&ctx.db)
    .await?;
    let now = ctx.clock.now();
    Ok(rows
        .into_iter()
        .map(|r| Person {
            id: r.id,
            username: r.username,
            avatar: r.avatar_key.map(|k| ctx.media.image(&k, now)),
        })
        .collect())
}

/// `(month, day)` pairs as `MM-DD` strings, the form the database compares birthdays in.
pub(crate) fn month_days(days: &[(u32, u32)]) -> Vec<String> {
    days.iter()
        .map(|(month, day)| format!("{month:02}-{day:02}"))
        .collect()
}
