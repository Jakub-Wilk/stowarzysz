//! Account management (administrators only; the HTTP layer checks that).

use chrono::{DateTime, NaiveDate, Utc};
use domain::DomainError;
use domain::accounts::{birthday, username};
use uuid::Uuid;

use super::stats;
use super::views::ManagedUser;
use crate::auth::{self, Actor};
use crate::clock::local_today;
use crate::db::Uow;
use crate::media::{self, Purpose};
use crate::{AppError, AppResult, Context};

struct Row {
    id: i64,
    username: String,
    is_active: bool,
    is_superuser: bool,
    has_password: bool,
    avatar_key: Option<String>,
    birthday: Option<NaiveDate>,
    created_at: DateTime<Utc>,
}

/// A new account. It has no password: the person sets one through an activation link.
#[derive(Debug, Clone)]
pub struct NewUser {
    pub username: String,
    pub is_active: bool,
    pub is_superuser: bool,
    pub birthday: Option<NaiveDate>,
}

/// Changes to an account; `None` leaves a field as it is. `birthday: Some(None)` clears it.
#[derive(Debug, Clone, Default)]
pub struct UserChanges {
    pub username: Option<String>,
    pub is_active: Option<bool>,
    pub is_superuser: Option<bool>,
    pub birthday: Option<Option<NaiveDate>>,
}

/// Every account, by username, with statistics (a constant number of queries).
pub async fn list(ctx: &Context) -> AppResult<Vec<ManagedUser>> {
    let rows = sqlx::query_as!(
        Row,
        r#"select id, username, is_active, is_superuser, password_hash is not null as "has_password!",
                  avatar_key, birthday, created_at
           from users order by username"#,
    )
    .fetch_all(&ctx.db)
    .await?;
    with_stats(ctx, rows).await
}

pub async fn get(ctx: &Context, id: i64) -> AppResult<ManagedUser> {
    let row = sqlx::query_as!(
        Row,
        r#"select id, username, is_active, is_superuser, password_hash is not null as "has_password!",
                  avatar_key, birthday, created_at
           from users where id = $1"#,
        id,
    )
    .fetch_optional(&ctx.db)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(with_stats(ctx, vec![row]).await?.remove(0))
}

async fn with_stats(ctx: &Context, rows: Vec<Row>) -> AppResult<Vec<ManagedUser>> {
    let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let mut voting = stats::voting(&ctx.db, &ids).await?;
    let mut pacts = stats::pacts(&ctx.db, &ids).await?;
    let now = ctx.clock.now();
    Ok(rows
        .into_iter()
        .map(|row| ManagedUser {
            id: row.id,
            username: row.username,
            is_active: row.is_active,
            is_superuser: row.is_superuser,
            has_password: row.has_password,
            avatar: row.avatar_key.map(|key| ctx.media.image(&key, now)),
            birthday: row.birthday,
            created_at: row.created_at,
            voting: voting.remove(&row.id).unwrap_or_default(),
            pacts: pacts.remove(&row.id).unwrap_or_default(),
        })
        .collect())
}

pub async fn create(ctx: &Context, user: NewUser) -> AppResult<ManagedUser> {
    let name = username::normalize(&user.username);
    let mut errors = Vec::new();
    errors.extend(username::validate(&name).err());
    if let Some(day) = user.birthday {
        errors.extend(birthday::validate(day, local_today(ctx.clock.now())).err());
    }
    if !errors.is_empty() {
        return Err(errors.into());
    }
    let id = sqlx::query_scalar!(
        "insert into users (username, is_active, is_superuser, birthday, created_at)
         values ($1, $2, $3, $4, $5) returning id",
        name,
        user.is_active,
        user.is_superuser,
        user.birthday,
        ctx.clock.now(),
    )
    .fetch_one(&ctx.db)
    .await
    .map_err(username_conflict)?;
    get(ctx, id).await
}

pub async fn update(
    ctx: &Context,
    actor: &Actor,
    id: i64,
    changes: UserChanges,
) -> AppResult<ManagedUser> {
    let mut errors = Vec::new();
    // An administrator locking themselves out would leave nobody to undo it.
    if id == actor.user_id {
        if changes.is_active == Some(false) {
            errors.push(
                DomainError::new(
                    "cannot_deactivate_self",
                    "Nie możesz dezaktywować samego siebie.",
                )
                .on("is_active"),
            );
        }
        if changes.is_superuser == Some(false) {
            errors.push(
                DomainError::new(
                    "cannot_demote_self",
                    "Nie możesz odebrać sobie uprawnień administratora.",
                )
                .on("is_superuser"),
            );
        }
    }
    let name = changes.username.as_deref().map(username::normalize);
    if let Some(name) = &name {
        errors.extend(username::validate(name).err());
    }
    if let Some(Some(day)) = changes.birthday {
        errors.extend(birthday::validate(day, local_today(ctx.clock.now())).err());
    }
    if !errors.is_empty() {
        return Err(errors.into());
    }

    let mut uow = Uow::begin(&ctx.db).await?;
    let was_active =
        sqlx::query_scalar!("select is_active from users where id = $1 for update", id)
            .fetch_optional(uow.conn())
            .await?
            .ok_or(AppError::NotFound)?;
    let (set_birthday, birthday) = match changes.birthday {
        Some(value) => (true, value),
        None => (false, None),
    };
    let is_active = sqlx::query_scalar!(
        "update users set
             username = coalesce($2, username),
             is_active = coalesce($3, is_active),
             is_superuser = coalesce($4, is_superuser),
             birthday = case when $5 then $6 else birthday end
         where id = $1 returning is_active",
        id,
        name,
        changes.is_active,
        changes.is_superuser,
        set_birthday,
        birthday,
    )
    .fetch_one(uow.conn())
    .await
    .map_err(username_conflict)?;
    if was_active && !is_active {
        // Deactivation signs the person out everywhere.
        auth::revoke(&mut uow, ctx, id, None).await?;
    }
    uow.commit().await?;
    get(ctx, id).await
}

/// Delete an account that has no history (e.g. created by mistake). Accounts that took part in
/// anything are deactivated instead, so nobody else's records change.
pub async fn delete(ctx: &Context, actor: &Actor, id: i64) -> AppResult<()> {
    if id == actor.user_id {
        return Err(
            DomainError::new("cannot_delete_self", "Nie możesz usunąć samego siebie.").into(),
        );
    }
    let mut uow = Uow::begin(&ctx.db).await?;
    let deleted = sqlx::query_scalar!("delete from users where id = $1 returning avatar_key", id)
        .fetch_optional(uow.conn())
        .await
        .map_err(|error| match &error {
            sqlx::Error::Database(db) if db.code().as_deref() == Some("23503") => {
                AppError::conflict(
                    "user_has_history",
                    "To konto ma już historię w aplikacji. Zamiast je usuwać, dezaktywuj je.",
                )
            }
            _ => error.into(),
        })?
        .ok_or(AppError::NotFound)?;
    media::trash(&mut uow, deleted.as_deref()).await?;
    uow.commit().await
}

/// Replace a user's profile picture with the caller's upload `upload_id`.
pub async fn set_avatar(
    ctx: &Context,
    actor: &Actor,
    id: i64,
    upload_id: Uuid,
) -> AppResult<ManagedUser> {
    let mut uow = Uow::begin(&ctx.db).await?;
    let old = lock_avatar(&mut uow, id).await?;
    let image =
        media::take_upload(&mut uow, ctx, actor.user_id, upload_id, Purpose::Avatar).await?;
    sqlx::query!(
        "update users set avatar_key = $2 where id = $1",
        id,
        image.key
    )
    .execute(uow.conn())
    .await?;
    media::trash(&mut uow, old.as_deref()).await?;
    uow.commit().await?;
    get(ctx, id).await
}

pub async fn clear_avatar(ctx: &Context, id: i64) -> AppResult<ManagedUser> {
    let mut uow = Uow::begin(&ctx.db).await?;
    let old = lock_avatar(&mut uow, id).await?;
    sqlx::query!("update users set avatar_key = null where id = $1", id)
        .execute(uow.conn())
        .await?;
    media::trash(&mut uow, old.as_deref()).await?;
    uow.commit().await?;
    get(ctx, id).await
}

async fn lock_avatar(uow: &mut Uow, id: i64) -> AppResult<Option<String>> {
    let row = sqlx::query!("select avatar_key from users where id = $1 for update", id)
        .fetch_optional(uow.conn())
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(row.avatar_key)
}

/// Sign a user out of every device.
pub async fn revoke_sessions(ctx: &Context, id: i64) -> AppResult<u64> {
    let mut uow = Uow::begin(&ctx.db).await?;
    let exists = sqlx::query_scalar!("select exists(select 1 from users where id = $1)", id)
        .fetch_one(uow.conn())
        .await?;
    if exists != Some(true) {
        return Err(AppError::NotFound);
    }
    let revoked = auth::revoke(&mut uow, ctx, id, None).await?;
    uow.commit().await?;
    Ok(revoked)
}

fn username_conflict(error: sqlx::Error) -> AppError {
    match &error {
        sqlx::Error::Database(db) if db.constraint() == Some("users_username_unique") => {
            username::taken().into()
        }
        _ => error.into(),
    }
}

/// The `create-superuser` command: an active administrator who can sign in straight away.
pub async fn create_superuser(ctx: &Context, raw_username: &str, password: &str) -> AppResult<i64> {
    let name = username::normalize(raw_username);
    let mut errors: Vec<DomainError> = username::validate(&name).err().into_iter().collect();
    errors.extend(domain::accounts::password::validate(password, &name));
    if !errors.is_empty() {
        return Err(errors.into());
    }
    let hash = ctx.passwords.hash(password).await?;
    sqlx::query_scalar!(
        "insert into users (username, password_hash, is_superuser, created_at)
         values ($1, $2, true, $3) returning id",
        name,
        hash,
        ctx.clock.now(),
    )
    .fetch_one(&ctx.db)
    .await
    .map_err(username_conflict)
}
