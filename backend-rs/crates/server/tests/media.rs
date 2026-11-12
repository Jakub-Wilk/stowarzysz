//! Uploads, signed media URLs, profile pictures, and the cleanup of everything temporary.
#![allow(clippy::unwrap_used)]

mod common;

use app::media::jobs::{MediaGc, MediaSweeper};
use app::scheduler::Job as _;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::TimeDelta;
use common::{Member, TestApp, png};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn upload(app: &TestApp, who: &Member, purpose: &str, width: u32, height: u32) -> Value {
    let response = app
        .upload(who, purpose, "image/png", png(width, height))
        .await;
    response.expect(StatusCode::CREATED);
    response.json()
}

fn webp_size(bytes: &[u8]) -> (u32, u32) {
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::WebP).unwrap();
    (image.width(), image.height())
}

fn key_of(url: &str) -> String {
    // /api/media/<prefix>/<uuid>/<file>?...
    let path = url
        .split('?')
        .next()
        .unwrap()
        .trim_start_matches("/api/media/");
    let mut parts = path.splitn(3, '/');
    format!("{}/{}", parts.next().unwrap(), parts.next().unwrap())
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn uploads_require_a_session(pool: PgPool) {
    let app = TestApp::new(pool);
    let request = Request::post("/api/uploads?purpose=photo")
        .header("content-type", "image/png")
        .body(Body::from(png(4, 4)))
        .unwrap();
    app.send(request).await.expect(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn an_upload_is_processed_and_served_through_signed_urls(pool: PgPool) {
    let app = TestApp::new(pool);
    let ala = app.member("ala").await;
    let uploaded = upload(&app, &ala, "photo", 4000, 1000).await;
    assert_eq!(
        (uploaded["width"].clone(), uploaded["height"].clone()),
        (json!(3072), json!(768))
    );
    assert_eq!(uploaded["purpose"], "photo");

    for (field, size) in [
        ("url", (3072, 768)),
        ("md", (1600, 400)),
        ("sm", (512, 128)),
    ] {
        let url = uploaded["image"][field].as_str().unwrap();
        let served = app.get(url).await; // no Authorization: the signature is the permission
        served.expect(StatusCode::OK);
        assert_eq!(served.header("content-type"), "image/webp");
        assert!(
            served
                .header("cache-control")
                .starts_with("private, max-age="),
            "{}",
            served.header("cache-control")
        );
        assert!(served.header("cache-control").ends_with("immutable"));
        assert_eq!(served.header("x-content-type-options"), "nosniff");
        assert_eq!(webp_size(&served.body), size, "{field}");
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn media_urls_cannot_be_forged_or_reused_forever(pool: PgPool) {
    let app = TestApp::new(pool);
    let ala = app.member("ala").await;
    let url = upload(&app, &ala, "avatar", 64, 64).await["image"]["sm"]
        .as_str()
        .unwrap()
        .to_owned();
    let forged = url.replace("sm.webp", "md.webp");
    let response = app.get(&forged).await;
    response.expect(StatusCode::FORBIDDEN);
    assert_eq!(response.code(), "invalid_media_link");
    app.get(url.split('?').next().unwrap())
        .await
        .expect(StatusCode::BAD_REQUEST); // no signature at all

    app.clock.advance(TimeDelta::days(1));
    app.get(&url).await.expect(StatusCode::OK); // valid for at least a day
    app.clock.advance(TimeDelta::days(2));
    app.get(&url).await.expect(StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn non_images_and_other_formats_are_refused(pool: PgPool) {
    let app = TestApp::new(pool);
    let ala = app.member("ala").await;
    let garbage = app
        .upload(&ala, "photo", "image/png", b"not really a png".to_vec())
        .await;
    garbage.expect(StatusCode::BAD_REQUEST);
    assert_eq!(garbage.code(), "invalid_image");
    assert_eq!(garbage.json()["detail"], "Prześlij prawidłowy obraz.");

    let bmp = app
        .upload(
            &ala,
            "photo",
            "image/bmp",
            b"BM\0\0\0\0\0\0\0\0\x36\0\0\0\x28\0\0\0".to_vec(),
        )
        .await;
    bmp.expect(StatusCode::UNSUPPORTED_MEDIA_TYPE);

    app.upload(&ala, "photo", "text/plain", png(4, 4))
        .await
        .expect(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let empty = app.upload(&ala, "photo", "image/png", Vec::new()).await;
    empty.expect(StatusCode::BAD_REQUEST);
    assert_eq!(empty.code(), "image_missing");
    app.upload(&ala, "selfie", "image/png", png(4, 4))
        .await
        .expect(StatusCode::BAD_REQUEST);
    // Nothing failed half-way leaves a file or a row behind.
    assert_eq!(app.count("select count(*) from uploads").await, 0);
    assert_eq!(
        std::fs::read_dir(app.ctx.media.tmp_dir()).unwrap().count(),
        0
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn uploads_over_the_limit_are_refused(pool: PgPool) {
    let app = TestApp::with_env(pool, &[("UPLOAD_LIMIT_BYTES", "100")]);
    let ala = app.member("ala").await;
    let response = app.upload(&ala, "photo", "image/png", png(200, 200)).await;
    response.expect(StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        std::fs::read_dir(app.ctx.media.tmp_dir()).unwrap().count(),
        0,
        "the partial upload is gone"
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_default_limit_is_50_mb(pool: PgPool) {
    let app = TestApp::new(pool);
    let ala = app.member("ala").await;
    let request = Request::post("/api/uploads?purpose=photo")
        .header("authorization", format!("Bearer {}", ala.token))
        .header("content-type", "image/jpeg")
        .header("content-length", (50 * 1024 * 1024 + 1).to_string())
        .body(Body::from(vec![0_u8; 16]))
        .unwrap();
    let response = app.send(request).await;
    response.expect(StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        response.json()["detail"],
        "Obraz jest za duży (maks. 50 MB)."
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn an_admin_sets_a_profile_picture_seen_everywhere(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let ala = app.member("ala").await;
    let uploaded = upload(&app, &admin, "avatar", 2000, 1200).await;
    let path = format!("/api/users/{}/avatar", ala.id);

    let managed = app
        .put_as(&admin, &path, json!({"upload_id": uploaded["id"]}))
        .await;
    let avatar = managed.expect(StatusCode::OK).json()["avatar"].clone();
    let key = key_of(avatar["url"].as_str().unwrap());
    assert_eq!(
        key,
        key_of(uploaded["image"]["url"].as_str().unwrap()),
        "the upload's files are used as they are"
    );
    assert_eq!(
        webp_size(&app.get(avatar["url"].as_str().unwrap()).await.body),
        (1024, 1024)
    );
    assert_eq!(
        app.count("select count(*) from uploads").await,
        0,
        "the upload was claimed"
    );

    assert_eq!(
        key_of(
            app.get_as(&ala, "/api/me").await.json()["avatar"]["sm"]
                .as_str()
                .unwrap()
        ),
        key
    );
    let login_users = app.get("/api/login-users").await.json();
    assert!(
        login_users
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["username"] == "ala" && !u["avatar"].is_null())
    );
    let people = app.get_as(&ala, "/api/people").await.json();
    assert!(
        people
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == ala.id && !p["avatar"].is_null())
    );

    // An upload is used once.
    let again = app
        .put_as(&admin, &path, json!({"upload_id": uploaded["id"]}))
        .await;
    again.expect(StatusCode::BAD_REQUEST);
    assert_eq!(again.code(), "upload_not_found");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn replaced_and_removed_pictures_are_deleted_after_commit(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let ala = app.member("ala").await;
    let path = format!("/api/users/{}/avatar", ala.id);
    let first = upload(&app, &admin, "avatar", 64, 64).await;
    app.put_as(&admin, &path, json!({"upload_id": first["id"]}))
        .await
        .expect(StatusCode::OK);
    let first_dir = app
        .ctx
        .media
        .dir(&key_of(first["image"]["url"].as_str().unwrap()));

    let second = upload(&app, &admin, "avatar", 64, 64).await;
    app.put_as(&admin, &path, json!({"upload_id": second["id"]}))
        .await
        .expect(StatusCode::OK);
    assert!(
        first_dir.exists(),
        "deleted by the sweeper, not inside the request"
    );
    MediaSweeper.run(&app.ctx, app.now()).await.unwrap();
    assert!(!first_dir.exists(), "the replaced picture is gone");

    let second_dir = app
        .ctx
        .media
        .dir(&key_of(second["image"]["url"].as_str().unwrap()));
    let cleared = app.delete_as(&admin, &path).await;
    assert_eq!(cleared.expect(StatusCode::OK).json()["avatar"], json!(null));
    MediaSweeper.run(&app.ctx, app.now()).await.unwrap();
    assert!(!second_dir.exists());
    assert_eq!(app.count("select count(*) from media_trash").await, 0);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn deleting_an_account_deletes_its_picture(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let ala = app.member("ala").await;
    let uploaded = upload(&app, &admin, "avatar", 64, 64).await;
    app.put_as(
        &admin,
        &format!("/api/users/{}/avatar", ala.id),
        json!({"upload_id": uploaded["id"]}),
    )
    .await
    .expect(StatusCode::OK);
    app.delete_as(&admin, &format!("/api/users/{}", ala.id))
        .await
        .expect(StatusCode::NO_CONTENT);
    MediaSweeper.run(&app.ctx, app.now()).await.unwrap();
    assert!(
        !app.ctx
            .media
            .dir(&key_of(uploaded["image"]["url"].as_str().unwrap()))
            .exists()
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn uploads_belong_to_their_owner_and_purpose(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let other = app.admin("drugi").await;
    let ala = app.member("ala").await;
    let path = format!("/api/users/{}/avatar", ala.id);
    let mine = upload(&app, &other, "avatar", 64, 64).await;
    app.put_as(&admin, &path, json!({"upload_id": mine["id"]}))
        .await
        .expect(StatusCode::BAD_REQUEST);
    let photo = upload(&app, &admin, "photo", 64, 64).await;
    app.put_as(&admin, &path, json!({"upload_id": photo["id"]}))
        .await
        .expect(StatusCode::BAD_REQUEST);
    app.put_as(
        &admin,
        "/api/users/999999/avatar",
        json!({"upload_id": photo["id"]}),
    )
    .await
    .expect(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn unclaimed_uploads_expire_after_30_minutes(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let ala = app.member("ala").await;
    let uploaded = upload(&app, &admin, "avatar", 64, 64).await;
    let dir = app
        .ctx
        .media
        .dir(&key_of(uploaded["image"]["url"].as_str().unwrap()));

    // The sweeper knows when the next upload expires.
    let next = MediaSweeper
        .run(&app.ctx, app.now())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next, app.now() + TimeDelta::minutes(30));
    assert!(dir.exists());

    app.clock.advance(TimeDelta::minutes(31));
    let admin = Member {
        token: app.session(admin.id).await.access,
        ..admin
    }; // 15-minute tokens
    let late = app
        .put_as(
            &admin,
            &format!("/api/users/{}/avatar", ala.id),
            json!({"upload_id": uploaded["id"]}),
        )
        .await;
    late.expect(StatusCode::BAD_REQUEST);
    MediaSweeper.run(&app.ctx, app.now()).await.unwrap();
    assert!(!dir.exists(), "deleted as soon as it expired");
    assert_eq!(app.count("select count(*) from uploads").await, 0);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_weekly_gc_removes_unreferenced_files_only(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let kept = upload(&app, &admin, "photo", 16, 16).await; // still an upload: referenced
    let orphan = app
        .ctx
        .media
        .dir("photos/00000000-0000-4000-8000-00000000dead");
    std::fs::create_dir_all(&orphan).unwrap();
    std::fs::write(orphan.join("master.webp"), b"x").unwrap();

    // Files younger than an hour are left alone (an upload may be mid-flight). File times are
    // real time, so this test uses real time too.
    MediaGc.run(&app.ctx, chrono::Utc::now()).await.unwrap();
    assert!(orphan.exists());
    sqlx::query("delete from scheduled_jobs")
        .execute(app.db())
        .await
        .unwrap();
    let later = chrono::Utc::now() + TimeDelta::hours(2);
    MediaGc.run(&app.ctx, later).await.unwrap();
    assert!(!orphan.exists(), "unreferenced and old: deleted");
    assert!(
        app.ctx
            .media
            .dir(&key_of(kept["image"]["url"].as_str().unwrap()))
            .exists()
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn uploads_are_rate_limited_per_user(pool: PgPool) {
    let app = TestApp::with_env(pool, &[("UPLOAD_RATE", "2/min")]);
    let ala = app.member("ala").await;
    let ola = app.member("ola").await;
    for _ in 0..2 {
        app.upload(&ala, "photo", "image/png", png(4, 4))
            .await
            .expect(StatusCode::CREATED);
    }
    app.upload(&ala, "photo", "image/png", png(4, 4))
        .await
        .expect(StatusCode::TOO_MANY_REQUESTS);
    app.upload(&ola, "photo", "image/png", png(4, 4))
        .await
        .expect(StatusCode::CREATED);
}
