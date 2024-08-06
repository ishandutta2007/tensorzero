mod migration_trait;
mod migrations;

use crate::clickhouse::ClickHouseConnectionInfo;
use crate::error::Error;
use migration_trait::Migration;
use migrations::migration_0000::Migration0000;

pub async fn run(clickhouse: &ClickHouseConnectionInfo) -> Result<(), Error> {
    run_migration(&Migration0000 { clickhouse }).await?;

    Ok(())
}

async fn run_migration(migration: &impl Migration) -> Result<(), Error> {
    migration.can_apply().await?;

    if migration.should_apply().await? {
        // Get the migration name (e.g. `Migration0000`)
        let migration_name = std::any::type_name_of_val(&migration)
            .split("::")
            .last()
            .unwrap_or("Unknown migration");

        tracing::info!("Applying migration: {migration_name}");

        if let Err(e) = migration.apply().await {
            tracing::error!(
                "Failed to apply migration: {migration_name}\n\n===== Rollback Instructions =====\n\n{}",
                migration.rollback_instructions()
            );
            return Err(e);
        }

        match migration.has_succeeded().await {
            Ok(true) => {
                tracing::info!("Migration succeeded: {migration_name}");
            }
            Ok(false) => {
                tracing::error!(
                    "Failed migration success check: {migration_name}\n\n===== Rollback Instructions =====\n\n{}",
                    migration.rollback_instructions()
                );
            }
            Err(e) => {
                tracing::error!(
                    "Failed to verify migration: {migration_name}\n\n===== Rollback Instructions =====\n\n{}",
                    migration.rollback_instructions()
                );
                return Err(e);
            }
        }
    }

    Ok(())
}

mod tests {
    use super::*;

    struct MockMigration {
        can_apply_result: bool,
        should_apply_result: bool,
        apply_result: bool,
        has_succeeded_result: bool,
        called_can_apply: std::sync::atomic::AtomicBool,
        called_should_apply: std::sync::atomic::AtomicBool,
        called_apply: std::sync::atomic::AtomicBool,
        called_has_succeeded: std::sync::atomic::AtomicBool,
    }

    impl Default for MockMigration {
        fn default() -> Self {
            Self {
                can_apply_result: true,
                should_apply_result: true,
                apply_result: true,
                has_succeeded_result: true,
                called_can_apply: std::sync::atomic::AtomicBool::new(false),
                called_should_apply: std::sync::atomic::AtomicBool::new(false),
                called_apply: std::sync::atomic::AtomicBool::new(false),
                called_has_succeeded: std::sync::atomic::AtomicBool::new(false),
            }
        }
    }

    impl Migration for MockMigration {
        async fn can_apply(&self) -> Result<(), Error> {
            self.called_can_apply
                .store(true, std::sync::atomic::Ordering::Relaxed);
            if self.can_apply_result {
                Ok(())
            } else {
                Err(Error::ClickHouseMigration {
                    id: "0000".to_string(),
                    message: "MockMigration can_apply failed".to_string(),
                })
            }
        }

        async fn should_apply(&self) -> Result<bool, Error> {
            self.called_should_apply
                .store(true, std::sync::atomic::Ordering::Relaxed);
            Ok(self.should_apply_result)
        }

        async fn apply(&self) -> Result<(), Error> {
            self.called_apply
                .store(true, std::sync::atomic::Ordering::Relaxed);
            if self.apply_result {
                Ok(())
            } else {
                Err(Error::ClickHouseMigration {
                    id: "0000".to_string(),
                    message: "MockMigration apply failed".to_string(),
                })
            }
        }

        async fn has_succeeded(&self) -> Result<bool, Error> {
            self.called_has_succeeded
                .store(true, std::sync::atomic::Ordering::Relaxed);
            Ok(self.has_succeeded_result)
        }

        fn rollback_instructions(&self) -> String {
            "".to_string()
        }
    }

    #[tokio::test]
    async fn test_run_migration_happy_path() {
        let mock_migration = MockMigration::default();

        // First check that method succeeds
        assert!(run_migration(&mock_migration).await.is_ok());

        // Check that we called every method
        assert!(mock_migration
            .called_can_apply
            .load(std::sync::atomic::Ordering::Relaxed));
        assert!(mock_migration
            .called_should_apply
            .load(std::sync::atomic::Ordering::Relaxed));
        assert!(mock_migration
            .called_apply
            .load(std::sync::atomic::Ordering::Relaxed));
        assert!(mock_migration
            .called_has_succeeded
            .load(std::sync::atomic::Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_run_migration_can_apply_fails() {
        let mock_migration = MockMigration {
            can_apply_result: false,
            ..Default::default()
        };

        // First check that the method fails
        assert!(run_migration(&mock_migration).await.is_err());

        // Check that we called every method
        assert!(mock_migration
            .called_can_apply
            .load(std::sync::atomic::Ordering::Relaxed));
        assert!(!mock_migration
            .called_should_apply
            .load(std::sync::atomic::Ordering::Relaxed));
        assert!(!mock_migration
            .called_apply
            .load(std::sync::atomic::Ordering::Relaxed));
        assert!(!mock_migration
            .called_has_succeeded
            .load(std::sync::atomic::Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_run_migration_should_apply_false() {
        let mock_migration = MockMigration {
            should_apply_result: false,
            ..Default::default()
        };

        // First check that the method succeeds
        assert!(run_migration(&mock_migration).await.is_ok());

        // Check that we called every method
        assert!(mock_migration
            .called_can_apply
            .load(std::sync::atomic::Ordering::Relaxed));
        assert!(mock_migration
            .called_should_apply
            .load(std::sync::atomic::Ordering::Relaxed));
        assert!(!mock_migration
            .called_apply
            .load(std::sync::atomic::Ordering::Relaxed));
        assert!(!mock_migration
            .called_has_succeeded
            .load(std::sync::atomic::Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_run_migration_apply_fails() {
        let mock_migration = MockMigration {
            apply_result: false,
            ..Default::default()
        };

        // First check that the method fails
        assert!(run_migration(&mock_migration).await.is_err());

        // Check that we called every method
        assert!(mock_migration
            .called_can_apply
            .load(std::sync::atomic::Ordering::Relaxed));
        assert!(mock_migration
            .called_should_apply
            .load(std::sync::atomic::Ordering::Relaxed));
        assert!(mock_migration
            .called_apply
            .load(std::sync::atomic::Ordering::Relaxed));
        assert!(!mock_migration
            .called_has_succeeded
            .load(std::sync::atomic::Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_run_migration_has_succeeded_false() {
        let mock_migration = MockMigration {
            has_succeeded_result: false,
            ..Default::default()
        };

        // First check that the method fails
        assert!(run_migration(&mock_migration).await.is_ok());

        // Check that we called every method
        assert!(mock_migration
            .called_can_apply
            .load(std::sync::atomic::Ordering::Relaxed));
        assert!(mock_migration
            .called_should_apply
            .load(std::sync::atomic::Ordering::Relaxed));
        assert!(mock_migration
            .called_apply
            .load(std::sync::atomic::Ordering::Relaxed));
        assert!(mock_migration
            .called_has_succeeded
            .load(std::sync::atomic::Ordering::Relaxed));
    }
}