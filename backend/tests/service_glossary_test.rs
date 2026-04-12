mod helpers;

use sqlx::PgPool;
use ops::middleware::auth::AuthUser;
use ops::services::glossary;
use ops::models::glossary::{CreateGlossaryRequest, UpdateGlossaryRequest};
use ops::error::AppError;
use uuid::Uuid;

fn super_admin() -> AuthUser {
    AuthUser {
        user_id: Uuid::new_v4(),
        role: "super_admin".to_string(),
        tenant_id: None,
        username: "test_admin".to_string(),
    }
}

fn member(tenant_id: Uuid) -> AuthUser {
    AuthUser {
        user_id: Uuid::new_v4(),
        role: "member".to_string(),
        tenant_id: Some(tenant_id),
        username: "test_member".to_string(),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_list_empty(pool: PgPool) {
    let admin = super_admin();
    let entries = glossary::list(&pool, &admin, None).await.unwrap();
    assert!(entries.is_empty());
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_and_list(pool: PgPool) {
    let admin = super_admin();

    let created = glossary::create(&pool, &admin, CreateGlossaryRequest {
        term: "ics".to_string(),
        full_name: Some("Inventory Control System".to_string()),
        description: Some("Manages warehouse inventory".to_string()),
        aliases: vec!["inventory".to_string()],
        aws_accounts: vec!["034362076319".to_string()],
        services: vec!["ecs".to_string()],
        account_id: None,
    }).await.unwrap();

    assert_eq!(created.term, "ics");
    assert_eq!(created.full_name, Some("Inventory Control System".to_string()));
    assert!(created.id != Uuid::nil());

    // List should return the created entry
    let entries = glossary::list(&pool, &admin, None).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, created.id);
    assert_eq!(entries[0].term, "ics");
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_empty_term_rejected(pool: PgPool) {
    let admin = super_admin();

    let result = glossary::create(&pool, &admin, CreateGlossaryRequest {
        term: "".to_string(),
        full_name: None,
        description: None,
        aliases: vec![],
        aws_accounts: vec![],
        services: vec![],
        account_id: None,
    }).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::BadRequest(msg) => {
            assert!(msg.contains("Term"), "Expected 'Term' in message, got: {}", msg);
        }
        other => panic!("Expected BadRequest, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_create_duplicate_term_conflict(pool: PgPool) {
    let admin = super_admin();

    glossary::create(&pool, &admin, CreateGlossaryRequest {
        term: "falcon".to_string(),
        full_name: Some("Falcon CI/CD".to_string()),
        description: None,
        aliases: vec![],
        aws_accounts: vec![],
        services: vec![],
        account_id: None,
    }).await.unwrap();

    let result = glossary::create(&pool, &admin, CreateGlossaryRequest {
        term: "falcon".to_string(),
        full_name: Some("Falcon Duplicate".to_string()),
        description: None,
        aliases: vec![],
        aws_accounts: vec![],
        services: vec![],
        account_id: None,
    }).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Conflict(msg) => {
            assert!(msg.contains("already exists"), "Expected 'already exists' in message, got: {}", msg);
        }
        other => panic!("Expected Conflict, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_delete_as_admin(pool: PgPool) {
    let admin = super_admin();

    let created = glossary::create(&pool, &admin, CreateGlossaryRequest {
        term: "delete-me".to_string(),
        full_name: None,
        description: None,
        aliases: vec![],
        aws_accounts: vec![],
        services: vec![],
        account_id: None,
    }).await.unwrap();

    glossary::delete(&pool, &admin, created.id).await.unwrap();

    // Verify it's gone
    let entries = glossary::list(&pool, &admin, None).await.unwrap();
    assert!(entries.is_empty());
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_delete_non_admin_no_account_forbidden(pool: PgPool) {
    let admin = super_admin();

    let created = glossary::create(&pool, &admin, CreateGlossaryRequest {
        term: "protected".to_string(),
        full_name: None,
        description: None,
        aliases: vec![],
        aws_accounts: vec![],
        services: vec![],
        account_id: None,
    }).await.unwrap();

    // A member (non-admin) should not be able to delete an entry with no account_id
    let member_user = member(Uuid::new_v4());
    let result = glossary::delete(&pool, &member_user, created.id).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Forbidden(_) => {}
        other => panic!("Expected Forbidden, got {:?}", other),
    }
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_update_success(pool: PgPool) {
    let admin = super_admin();

    let created = glossary::create(&pool, &admin, CreateGlossaryRequest {
        term: "API".to_string(),
        full_name: Some("Application Programming Interface".to_string()),
        description: None,
        aliases: vec![],
        aws_accounts: vec![],
        services: vec![],
        account_id: None,
    }).await.unwrap();

    let updated = glossary::update(&pool, &admin, created.id, UpdateGlossaryRequest {
        term: Some("REST API".to_string()),
        full_name: None,
        description: Some("RESTful API endpoint".to_string()),
        aliases: None,
        aws_accounts: None,
        services: None,
        account_id: None,
    }).await.unwrap();

    assert_eq!(updated.id, created.id);
    assert_eq!(updated.term, "REST API");
    assert_eq!(updated.full_name, Some("Application Programming Interface".to_string())); // unchanged
    assert_eq!(updated.description, Some("RESTful API endpoint".to_string()));

    // Verify persisted by re-listing
    let entries = glossary::list(&pool, &admin, None).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].term, "REST API");
}

#[sqlx::test(migrations = "src/migrations")]
async fn test_update_not_found(pool: PgPool) {
    let admin = super_admin();

    let result = glossary::update(&pool, &admin, Uuid::new_v4(), UpdateGlossaryRequest {
        term: Some("Ghost".to_string()),
        full_name: None,
        description: None,
        aliases: None,
        aws_accounts: None,
        services: None,
        account_id: None,
    }).await;

    assert!(matches!(result, Err(AppError::NotFound(_))));
}
