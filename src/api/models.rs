use crate::auth;

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct ModelError {
    pub code: &'static str,
    pub message: &'static str,
}

pub fn validate_models(allowed: &[String], providers: &[String]) -> Result<(), ModelError> {
    for model in allowed {
        if !auth::valid_model(model) {
            return Err(ModelError {
                code: "invalid_model",
                message: "invalid configured model",
            });
        }
        let (provider, name) = model.split_once('/').unwrap();
        if name == "default" || name.ends_with("/default") {
            return Err(ModelError {
                code: "invalid_model",
                message: "dynamic default models are not allowed in a strict catalog",
            });
        }
        if !providers.iter().any(|p| p == provider) {
            return Err(ModelError {
                code: "model_not_found",
                message: "provider is not enabled",
            });
        }
    }
    Ok(())
}
pub fn allows(scopes: &[String], model: &str) -> bool {
    scopes.iter().any(|s| {
        s == "*"
            || s == model
            || s.strip_suffix('*')
                .filter(|_| s.ends_with("/*"))
                .is_some_and(|prefix| model.starts_with(prefix))
    })
}
pub fn check_model(
    model: &str,
    scopes: &[String],
    providers: &[String],
    allowed: &[String],
) -> Result<(), ModelError> {
    if !auth::valid_model(model) {
        return Err(ModelError {
            code: "invalid_model",
            message: "model must be provider/model",
        });
    }
    if !allows(scopes, model) {
        return Err(ModelError {
            code: "permission_denied",
            message: "key cannot use this model",
        });
    }
    let provider = model.split_once('/').unwrap().0;
    if !providers.iter().any(|p| p == provider) {
        return Err(ModelError {
            code: "model_not_found",
            message: "provider is not enabled",
        });
    }
    if !allowed.is_empty() && !allowed.iter().any(|m| m == model) {
        return Err(ModelError {
            code: "model_not_found",
            message: "model is not in the configured catalog",
        });
    }
    Ok(())
}
