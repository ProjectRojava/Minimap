use minimap_store::StoreError;
use minimap_types::AppError;

/// Builds the `{ code, message }` error the frontend receives.
pub fn app_error(code: &str, message: impl std::fmt::Display) -> AppError {
    AppError {
        code: code.to_owned(),
        message: message.to_string(),
    }
}

pub fn store_error(e: StoreError) -> AppError {
    let code = match &e {
        StoreError::NotFound { .. } | StoreError::EdgeNotFound(_) => "not_found",
        StoreError::Invalid(_) => "invalid",
        StoreError::Constraint(_) => "constraint",
        StoreError::DuplicateEdge => "duplicate",
        StoreError::WrongKey => "wrong_key",
        StoreError::BackupKeyNeeded => "backup_key_needed",
        StoreError::NewerData { .. } => "newer_data",
        StoreError::NotArchived { .. }
        | StoreError::AlreadyArchived { .. }
        | StoreError::NotArchivedYet { .. } => "state",
        _ => "store",
    };
    app_error(code, e)
}

pub fn rule_error(e: minimap_core::edge_rules::EdgeRuleError) -> AppError {
    app_error("invalid_edge", e)
}

/// "Can't add: this would create a loop — A → B → A".
pub fn cycle_error(what: &str, labels: &[String]) -> AppError {
    app_error(
        "cycle",
        format!(
            "Can't {what}: this would create a loop — {}",
            labels.join(" → ")
        ),
    )
}
