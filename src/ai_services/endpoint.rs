use url::Url;

use super::error::AiError;

pub fn normalize_service_endpoint(value: &str) -> Result<String, AiError> {
    let parsed =
        Url::parse(value.trim()).map_err(|_| AiError::Validation("服务地址无效".to_owned()))?;
    validate_common(&parsed, "服务地址不能包含凭据或查询参数")?;
    let host = parsed
        .host_str()
        .ok_or_else(|| AiError::Validation("服务地址缺少主机".to_owned()))?;
    let loopback = host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1";
    if parsed.scheme() != "https" && !(parsed.scheme() == "http" && loopback) {
        return Err(AiError::Validation(
            "远程服务必须使用 HTTPS；HTTP 只允许本机回环地址".to_owned(),
        ));
    }
    Ok(parsed.to_string().trim_end_matches('/').to_owned())
}

pub fn normalize_proxy_url(value: Option<&str>) -> Result<Option<String>, AiError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let parsed = Url::parse(value)
        .map_err(|_| AiError::Validation("请输入完整的 HTTP(S) 代理地址".to_owned()))?;
    validate_common(&parsed, "代理地址不能包含账号、密码或查询参数")?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(AiError::Validation(
            "请输入完整的 HTTP(S) 代理地址".to_owned(),
        ));
    }
    Ok(Some(parsed.to_string().trim_end_matches('/').to_owned()))
}

fn validate_common(parsed: &Url, message: &str) -> Result<(), AiError> {
    if !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        Err(AiError::Validation(message.to_owned()))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_http_is_rejected_and_loopback_http_is_allowed() {
        assert!(normalize_service_endpoint("http://api.example.test").is_err());
        assert_eq!(
            normalize_service_endpoint("http://127.0.0.1:8080/").unwrap(),
            "http://127.0.0.1:8080"
        );
    }

    #[test]
    fn credentials_and_queries_are_rejected() {
        assert!(normalize_service_endpoint("https://user:secret@example.test").is_err());
        assert!(normalize_proxy_url(Some("http://proxy.test?q=secret")).is_err());
    }
}
