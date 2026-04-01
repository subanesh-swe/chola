use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Chola CI API",
        version = "1.0.0",
        description = "CI/CD Orchestrator REST API"
    ),
    tags(
        (name = "Auth", description = "Authentication"),
        (name = "Users", description = "User management"),
        (name = "Repos", description = "Repository management"),
        (name = "Builds", description = "Build/job group management"),
        (name = "Jobs", description = "Job management"),
        (name = "Workers", description = "Worker management"),
        (name = "Dashboard", description = "Dashboard summary"),
        (name = "Analytics", description = "Build analytics"),
        (name = "Variables", description = "Pipeline variables"),
        (name = "Webhooks", description = "Webhook management"),
        (name = "Blacklist", description = "Command and branch blacklists"),
        (name = "Schedules", description = "Cron schedules"),
        (name = "Notifications", description = "Notification configs"),
        (name = "Scripts", description = "Stage scripts"),
        (name = "Settings", description = "System settings"),
        (name = "Audit", description = "Audit log"),
        (name = "API Keys", description = "API key management"),
        (name = "Artifacts", description = "Build artifacts"),
        (name = "Badge", description = "Build status badges"),
        (name = "Health", description = "Health checks"),
    ),
    paths(
        crate::api::auth_handlers::login,
        crate::api::auth_handlers::me,
        crate::api::repo_handlers::list,
        crate::api::job_group_handlers::list,
        crate::api::job_group_handlers::trigger,
        crate::api::worker_handlers::list,
        crate::api::dashboard_handlers::summary,
        crate::api::analytics_handlers::get_analytics,
        crate::api::blacklist_handlers::list_command_blacklist,
    ),
    components(schemas(
        ci_core::models::stage::Repo,
        ci_core::models::stage::StageConfig,
        ci_core::models::stage::StageScript,
        ci_core::models::stage::Webhook,
        ci_core::models::stage::WorkerReservation,
        ci_core::models::job::Job,
        ci_core::models::job::JobState,
        ci_core::models::job::JobType,
        ci_core::models::job_group::JobGroup,
        ci_core::models::job_group::JobGroupState,
        ci_core::models::worker::WorkerInfo,
        ci_core::models::worker::WorkerHeartbeat,
        ci_core::models::worker::WorkerState,
        ci_core::models::worker::WorkerStatus,
        ci_core::models::worker::DiskType,
        ci_core::models::user::User,
        ci_core::models::user::UserRole,
        crate::api::auth_handlers::LoginRequest,
        crate::api::auth_handlers::LoginResponse,
        crate::api::auth_handlers::UserResponse,
        crate::api::job_group_handlers::TriggerRequest,
    )),
    modifiers(&SecurityAddon)
)]
pub struct ApiDoc;

struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer_auth",
            utoipa::openapi::security::SecurityScheme::Http(utoipa::openapi::security::Http::new(
                utoipa::openapi::security::HttpAuthScheme::Bearer,
            )),
        );
    }
}
