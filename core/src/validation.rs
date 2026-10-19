#![allow(dead_code)]

use crate::AppError;

pub struct VaultValidator;

impl VaultValidator {
    pub fn validate_vault_config(config_json: &str) -> Result<(), AppError> {
        if config_json.is_empty() {
            return Err(AppError::BadRequest("vault config must not be empty".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_empty_config() {
        assert!(VaultValidator::validate_vault_config("").is_err());
    }

    #[test]
    fn test_validate_valid_config() {
        assert!(VaultValidator::validate_vault_config("{}").is_ok());
    }
}
