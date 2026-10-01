use crate::{
    AdvancedConfig, AppDraft, AppTemplate, EnvironmentVariable, ManagedApp, ProcessConfig,
    RestartPolicy, validation::validate_app, winsw::render_service_config,
};

fn draft() -> AppDraft {
    AppDraft {
        name: "Night bridge".to_owned(),
        process: ProcessConfig {
            executable: "C:\\Program Files\\桥接服务\\server.exe".to_owned(),
            arguments: "--name \"Night shift\" --note \"one & two < three\"".to_owned(),
            working_directory: "C:\\Program Files\\桥接服务".to_owned(),
            environment: vec![EnvironmentVariable {
                name: "BRIDGE_REGION".to_owned(),
                value: "east & north".to_owned(),
            }],
        },
        startup_enabled: true,
        restart: AppTemplate::BackgroundApplication.defaults().restart,
        logging_enabled: true,
        advanced: AdvancedConfig {
            stop_timeout_seconds: 15,
        },
    }
}

fn test_app() -> ManagedApp {
    draft().into_app("S-1-5-21-1001-1002-1003-1000".to_owned())
}

#[test]
fn service_identifier_is_stable_when_display_name_changes() {
    let mut app = test_app();
    let service_id = app.service_id();
    let id = app.id;

    app.replace_from(
        AppDraft {
            name: "Renamed bridge".to_owned(),
            ..draft()
        }
        .into_app("S-1-5-21-9999".to_owned()),
    );

    assert_eq!(app.service_id(), service_id);
    assert_eq!(app.id, id);
    assert_eq!(app.owner_sid, "S-1-5-21-1001-1002-1003-1000");
    assert_eq!(app.revision, 2);
}

#[test]
fn managed_app_configuration_round_trips_as_json() {
    let app = test_app();
    let encoded = serde_json::to_string(&app).expect("the app configuration should serialize");
    let decoded: ManagedApp =
        serde_json::from_str(&encoded).expect("the app configuration should deserialize");

    assert_eq!(decoded, app);
}

#[test]
fn templates_choose_safe_default_behaviour() {
    let default = AppTemplate::LocalServer.defaults();
    let custom = AppTemplate::Custom.defaults();

    assert!(default.startup_enabled);
    assert_eq!(default.restart.policy, RestartPolicy::OnFailure);
    assert_eq!(default.restart.delay_seconds, 5);
    assert!(default.logging_enabled);
    assert!(!custom.startup_enabled);
    assert_eq!(custom.restart.policy, RestartPolicy::Never);
    assert!(custom.logging_enabled);
}

#[test]
fn service_xml_escapes_user_input_and_limits_privilege() {
    let xml = render_service_config(
        &test_app(),
        r"C:\ProgramData\StayUp\Logs\owner\id",
        "D:(A;;RCLRPWP;;;S-1-5-21-1001-1002-1003-1000)",
    )
    .expect("service XML should serialize");

    assert!(xml.contains("<service>"));
    assert!(xml.contains("<executable>C:\\Program Files\\桥接服务\\server.exe</executable>"));
    assert!(xml.contains("<workingdirectory>C:\\Program Files\\桥接服务</workingdirectory>"));
    assert!(xml.contains(r#"--name "Night shift""#));
    assert!(xml.contains("one &amp; two &lt; three"));
    assert!(xml.contains("east &amp; north"));
    assert!(xml.contains("<user>LocalService</user>"));
    assert!(xml.contains("<onfailure action=\"restart\" delay=\"5 sec\""));
    assert!(xml.contains("<sizeThreshold>10240</sizeThreshold>"));
    assert!(xml.contains("<keepFiles>5</keepFiles>"));
    assert!(xml.contains("<startmode>Automatic</startmode>"));
    assert!(xml.contains(
        "<securityDescriptor>D:(A;;RCLRPWP;;;S-1-5-21-1001-1002-1003-1000)</securityDescriptor>"
    ));
}

#[test]
fn disabling_restart_maps_to_an_explicit_no_action() {
    let mut app = test_app();
    app.restart.policy = RestartPolicy::Never;

    let xml = render_service_config(&app, r"C:\ProgramData\StayUp\logs", "D:(A;;GA;;;SY)")
        .expect("service XML should serialize");

    assert!(xml.contains("<onfailure action=\"none\""));
}

#[test]
fn validation_rejects_duplicate_environment_names_ignoring_case() {
    let mut app = test_app();
    app.process.environment = vec![
        EnvironmentVariable {
            name: "APP_MODE".to_owned(),
            value: "production".to_owned(),
        },
        EnvironmentVariable {
            name: "app_mode".to_owned(),
            value: "test".to_owned(),
        },
    ];

    assert_eq!(
        validate_app(&app, false).unwrap_err().code,
        "invalid_environment_variable"
    );
}

#[test]
fn validation_rejects_runtime_variables_and_network_paths() {
    let mut app = test_app();
    app.process.environment = vec![EnvironmentVariable {
        name: "WINSW_EXECUTABLE".to_owned(),
        value: "custom".to_owned(),
    }];
    assert_eq!(
        validate_app(&app, false).unwrap_err().code,
        "reserved_environment_variable"
    );

    app = test_app();
    app.process.executable = r"\\server\share\server.exe".to_owned();
    assert_eq!(
        validate_app(&app, false).unwrap_err().code,
        "unsupported_executable_path"
    );
}

#[test]
fn validation_rejects_xml_control_characters_and_oversized_configuration() {
    let mut app = test_app();
    app.name.push('\u{1}');
    assert_eq!(validate_app(&app, false).unwrap_err().code, "invalid_name");

    app = test_app();
    app.process.environment = (0..34)
        .map(|index| EnvironmentVariable {
            name: format!("VALUE_{index}"),
            value: "x".repeat(32_000),
        })
        .collect();
    assert_eq!(
        validate_app(&app, false).unwrap_err().code,
        "configuration_too_large"
    );
}
