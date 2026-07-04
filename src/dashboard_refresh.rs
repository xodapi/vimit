use crate::{
    DEFAULT_API_BASE, DEFAULT_DANGER_THRESHOLD, DEFAULT_WARNING_THRESHOLD, Dashboard, HttpClient,
    Router, RuntimeConfig, demo_payload, load_mock, summarize_me,
};

#[derive(Debug, Clone)]
pub struct DashboardRefreshResult {
    pub dashboard: Dashboard,
    pub active_endpoint_label: String,
    pub live_api_key_present: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct DashboardRefreshRequest<'a> {
    pub force_demo: bool,
    pub mock_path: Option<&'a str>,
    pub warning_threshold: f64,
    pub danger_threshold: f64,
    pub missing_api_key_source: &'a str,
}

impl<'a> Default for DashboardRefreshRequest<'a> {
    fn default() -> Self {
        Self {
            force_demo: false,
            mock_path: None,
            warning_threshold: DEFAULT_WARNING_THRESHOLD,
            danger_threshold: DEFAULT_DANGER_THRESHOLD,
            missing_api_key_source: "источник: демо; добавьте VIBEMODE_API_KEY в .env для live-лимитов",
        }
    }
}

pub fn load_dashboard_refresh(
    request: DashboardRefreshRequest<'_>,
    config: &RuntimeConfig,
    http: &HttpClient,
    router: &mut Router,
) -> Result<DashboardRefreshResult, String> {
    let (payload, source, active_endpoint_label, live_api_key_present) = if request.force_demo {
        (
            demo_payload(),
            "источник: встроенные демо-данные".to_string(),
            "demo".to_string(),
            false,
        )
    } else if let Some(path) = request.mock_path {
        (
            load_mock(path)?,
            format!("источник: mock {path}"),
            "mock".to_string(),
            false,
        )
    } else if config.api_key.is_empty() {
        (
            demo_payload(),
            request.missing_api_key_source.to_string(),
            "demo".to_string(),
            false,
        )
    } else {
        let (payload, label) =
            http.fetch_me_with_retry(&config.api_key, router, &config.api_base)?;
        let endpoint = dashboard_endpoint_for_label(&label, &config.api_base);
        (
            payload,
            format!("источник: live VibeMode /v1/me на {endpoint}"),
            label,
            true,
        )
    };

    let windows = summarize_me(
        &payload,
        request.warning_threshold,
        request.danger_threshold,
    );
    let status = crate::dashboard_status(&windows);

    Ok(DashboardRefreshResult {
        dashboard: Dashboard {
            source,
            status,
            agent: String::new(),
            token_rate: String::new(),
            windows,
            daily: None,
        },
        active_endpoint_label,
        live_api_key_present,
    })
}

pub fn dashboard_endpoint_for_label(label: &str, configured_api_base: &str) -> String {
    match label {
        "api" => DEFAULT_API_BASE.to_string(),
        "r-api" => crate::FALLBACK_API_BASE.to_string(),
        _ => configured_api_base.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_dashboard_refresh_falls_back_to_demo_without_api_key() {
        let http = HttpClient::new(crate::USER_AGENT).unwrap();
        let mut router = Router::new(
            crate::DEFAULT_API_BASE.to_string(),
            crate::api_fallbacks_for(crate::DEFAULT_API_BASE, true),
        );
        let config = RuntimeConfig {
            api_base: crate::DEFAULT_API_BASE.to_string(),
            api_key: String::new(),
            abtop_bin: String::new(),
            auto_failover: true,
        };

        let result = load_dashboard_refresh(
            DashboardRefreshRequest {
                missing_api_key_source:
                    "источник: демо; добавьте VIBEMODE_API_KEY в .env для live-лимитов",
                ..DashboardRefreshRequest::default()
            },
            &config,
            &http,
            &mut router,
        )
        .unwrap();

        assert_eq!(result.active_endpoint_label, "demo");
        assert!(!result.live_api_key_present);
        assert_eq!(
            result.dashboard.source,
            "источник: демо; добавьте VIBEMODE_API_KEY в .env для live-лимитов"
        );
        assert!(!result.dashboard.windows.is_empty());
    }

    #[test]
    fn dashboard_endpoint_for_label_maps_known_hosts() {
        assert_eq!(
            dashboard_endpoint_for_label("api", "https://custom.example"),
            crate::DEFAULT_API_BASE
        );
        assert_eq!(
            dashboard_endpoint_for_label("r-api", "https://custom.example"),
            crate::FALLBACK_API_BASE
        );
        assert_eq!(
            dashboard_endpoint_for_label("custom", "https://custom.example"),
            "https://custom.example"
        );
    }
}
