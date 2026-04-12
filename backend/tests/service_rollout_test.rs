mod helpers;

use sqlx::PgPool;
use uuid::Uuid;

use openops::middleware::auth::AuthUser;
use openops::services::rollout;

async fn seed_tenant(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ('t', 't') RETURNING id")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_cluster(pool: &PgPool, tenant_id: Option<Uuid>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO clusters (name, cloud, tenant_id) VALUES ('test-cluster', 'aws', $1) RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_user(pool: &PgPool, username: &str, role: &str, tenant_id: Option<Uuid>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (username, password_hash, role, tenant_id) VALUES ($1, 'hash', $2, $3) RETURNING id",
    )
    .bind(username)
    .bind(role)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn super_admin(pool: &PgPool) -> AuthUser {
    let user_id = seed_user(pool, "admin", "super_admin", None).await;
    AuthUser {
        user_id,
        role: "super_admin".to_string(),
        tenant_id: None,
        username: "admin".to_string(),
    }
}

async fn member(pool: &PgPool, tenant_id: Uuid) -> AuthUser {
    let user_id = seed_user(pool, &format!("m-{}", Uuid::new_v4()), "member", Some(tenant_id)).await;
    AuthUser {
        user_id,
        role: "member".to_string(),
        tenant_id: Some(tenant_id),
        username: "m".to_string(),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_record_and_list_events(pool: PgPool) {
    let tid = seed_tenant(&pool).await;
    let cluster_id = seed_cluster(&pool, Some(tid)).await;
    let admin = super_admin(&pool).await;

    // Record an event
    rollout::record_event(
        &pool,
        cluster_id,
        "default",
        "my-rollout",
        "promote_step",
        serde_json::json!({"full": false}),
        Some(admin.user_id),
        Some(tid),
    )
    .await;

    // List events — super_admin sees all (tenant_id is None → no tenant filter)
    let events = rollout::list_events(&pool, &admin, Some(cluster_id), None, None)
        .await
        .unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].cluster_id, cluster_id);
    assert_eq!(events[0].namespace, "default");
    assert_eq!(events[0].rollout_name, "my-rollout");
    assert_eq!(events[0].action, "promote_step");
    assert_eq!(events[0].tenant_id, Some(tid));
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_list_events_tenant_filtered(pool: PgPool) {
    let tid1 = seed_tenant(&pool).await;
    let tid2: Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ('t2', 't2') RETURNING id")
            .fetch_one(&pool)
            .await
            .unwrap();

    let cluster1 = seed_cluster(&pool, Some(tid1)).await;
    let cluster2 = seed_cluster(&pool, Some(tid2)).await;

    // Record event for tenant 1
    rollout::record_event(
        &pool,
        cluster1,
        "ns1",
        "rollout-a",
        "promote_full",
        serde_json::json!({}),
        None,
        Some(tid1),
    )
    .await;

    // Record event for tenant 2
    rollout::record_event(
        &pool,
        cluster2,
        "ns2",
        "rollout-b",
        "rollback",
        serde_json::json!({}),
        None,
        Some(tid2),
    )
    .await;

    // Member of tenant 1 should only see tenant 1's events
    let user1 = member(&pool, tid1).await;
    let events1 = rollout::list_events(&pool, &user1, None, None, None)
        .await
        .unwrap();
    assert_eq!(events1.len(), 1);
    assert_eq!(events1[0].rollout_name, "rollout-a");

    // Member of tenant 2 should only see tenant 2's events
    let user2 = member(&pool, tid2).await;
    let events2 = rollout::list_events(&pool, &user2, None, None, None)
        .await
        .unwrap();
    assert_eq!(events2.len(), 1);
    assert_eq!(events2[0].rollout_name, "rollout-b");

    // Super admin sees all events (no tenant filter)
    let admin = super_admin(&pool).await;
    let all_events = rollout::list_events(&pool, &admin, None, None, None)
        .await
        .unwrap();
    assert_eq!(all_events.len(), 2);
}
