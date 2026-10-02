use minimap_types::AppError;

/// Builds the `{ code, message }` error the frontend receives.
pub fn app_error(code: &str, message: impl std::fmt::Display) -> AppError {
    AppError {
        code: code.to_owned(),
        message: message.to_string(),
    }
}
