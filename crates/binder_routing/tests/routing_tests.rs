//! Comprehensive integration tests for the selective routing policy engine.

use binder_routing::{
    CodeFilter, DescriptorMatcher, MatchRule, MatcherEngine, RouteAction, RoutingPolicy,
    RoutingRule, ServiceNameMatcher,
};

#[test]
fn test_default_local_deny_unregistered_interfaces() {
    let policy = RoutingPolicy::new_default_local();
    assert!(policy.is_empty());
    assert_eq!(policy.len(), 0);
    assert_eq!(policy.default_action(), &RouteAction::LocalGuest);

    // Any unconfigured interface descriptor should route to LocalGuest
    assert_eq!(
        policy.route("android.os.IServiceManager", 1),
        RouteAction::LocalGuest
    );
    assert_eq!(
        policy.route("android.view.IWindowManager", 100),
        RouteAction::LocalGuest
    );
    assert_eq!(
        policy.route("android.hardware.graphics.composer.IComposer", 5),
        RouteAction::LocalGuest
    );
}

#[test]
fn test_host_offload_whitelist() {
    let mut policy = RoutingPolicy::new_default_local();
    policy.allow_host_offload("android.gui.ISurfaceComposer");
    policy.allow_host_offload("android.gui.IGraphicBufferProducer");

    assert_eq!(policy.len(), 2);
    assert!(!policy.is_empty());

    // Whitelisted interfaces route to HostOffload regardless of transaction code
    for code in [1, 1006, 1010, 1020, 1025, 0x5f504e47] {
        assert_eq!(
            policy.route("android.gui.ISurfaceComposer", code),
            RouteAction::HostOffload
        );
        assert_eq!(
            policy.route("android.gui.IGraphicBufferProducer", code),
            RouteAction::HostOffload
        );
    }

    // Non-whitelisted interface routes to LocalGuest
    assert_eq!(
        policy.route("android.gui.IDisplayEventConnection", 1),
        RouteAction::LocalGuest
    );
}

#[test]
fn test_hybrid_routing_opcodes() {
    let mut policy = RoutingPolicy::new_default_local();
    // Only route CREATE_SURFACE (1006) and SET_TRANSACTION_STATE (1020) to host
    let target_codes = vec![1006, 1020];
    policy.allow_hybrid("android.gui.ISurfaceComposer", target_codes);

    assert_eq!(
        policy.route("android.gui.ISurfaceComposer", 1006),
        RouteAction::HostOffload
    );
    assert_eq!(
        policy.route("android.gui.ISurfaceComposer", 1020),
        RouteAction::HostOffload
    );
    assert_eq!(
        policy.route("android.gui.ISurfaceComposer", 1010), // GET_DISPLAY_INFO -> local
        RouteAction::LocalGuest
    );
    assert_eq!(
        policy.route("android.gui.ISurfaceComposer", 1025), // BOOT_FINISHED -> local
        RouteAction::LocalGuest
    );
}

#[test]
fn test_rule_replacement_and_removal() {
    let mut policy = RoutingPolicy::new_default_local();
    policy.allow_host_offload("android.gui.ISurfaceComposer");
    assert_eq!(
        policy.route("android.gui.ISurfaceComposer", 1006),
        RouteAction::HostOffload
    );

    // Update rule to Hybrid
    policy.allow_hybrid("android.gui.ISurfaceComposer", vec![1006]);
    assert_eq!(
        policy.route("android.gui.ISurfaceComposer", 1006),
        RouteAction::HostOffload
    );
    assert_eq!(
        policy.route("android.gui.ISurfaceComposer", 1020),
        RouteAction::LocalGuest
    );

    // Remove rule
    let removed = policy.remove_rule("android.gui.ISurfaceComposer");
    assert!(removed.is_some());
    assert_eq!(
        policy.route("android.gui.ISurfaceComposer", 1006),
        RouteAction::LocalGuest
    );
}

#[test]
fn test_wildcard_descriptor_matching() {
    let mut policy = RoutingPolicy::new_default_local();
    policy.add_rule(RoutingRule::new("android.gui.*", RouteAction::HostOffload));

    assert_eq!(
        policy.route("android.gui.ISurfaceComposer", 1006),
        RouteAction::HostOffload
    );
    assert_eq!(
        policy.route("android.gui.IGraphicBufferProducer", 1),
        RouteAction::HostOffload
    );
    assert_eq!(
        policy.route("android.gui.IDisplayEventConnection", 2),
        RouteAction::HostOffload
    );
    assert_eq!(
        policy.route("android.os.IServiceManager", 1),
        RouteAction::LocalGuest
    );
}

#[test]
fn test_service_name_matching_and_priority() {
    let mut policy = RoutingPolicy::new_default_local();

    // Default SurfaceFlinger service offloaded with priority 10
    policy.add_rule(
        RoutingRule::new("android.gui.ISurfaceComposer", RouteAction::HostOffload)
            .with_service_name("SurfaceFlinger")
            .with_priority(10),
    );

    // Fallback rule for ISurfaceComposer to LocalGuest with priority 0
    policy.add_rule(
        RoutingRule::new("android.gui.ISurfaceComposer", RouteAction::LocalGuest)
            .with_priority(0),
    );

    assert_eq!(
        policy.route_service(Some("SurfaceFlinger"), "android.gui.ISurfaceComposer", 1006),
        RouteAction::HostOffload
    );
    assert_eq!(
        policy.route_service(Some("OtherService"), "android.gui.ISurfaceComposer", 1006),
        RouteAction::LocalGuest
    );
}

#[test]
fn test_matcher_engine_complex_filters() {
    let mut engine = MatcherEngine::new(RouteAction::LocalGuest);

    // Range rule: opcodes 1000..=1050 for SurfaceFlinger
    let range_rule = MatchRule::new(
        DescriptorMatcher::Exact("android.gui.ISurfaceComposer".to_string()),
        RouteAction::HostOffload,
    )
    .with_service(ServiceNameMatcher::Exact("SurfaceFlinger".to_string()))
    .with_code_filter(CodeFilter::Range(1000, 1050))
    .with_priority(100);
    engine.add_rule(range_rule);

    assert_eq!(
        engine.match_transaction(
            Some("SurfaceFlinger"),
            Some("android.gui.ISurfaceComposer"),
            1006
        ),
        RouteAction::HostOffload
    );
    assert_eq!(
        engine.match_transaction(
            Some("SurfaceFlinger"),
            Some("android.gui.ISurfaceComposer"),
            1051
        ),
        RouteAction::LocalGuest
    );
    assert_eq!(
        engine.match_transaction(
            Some("Unknown"),
            Some("android.gui.ISurfaceComposer"),
            1006
        ),
        RouteAction::LocalGuest
    );
}

#[test]
fn test_policy_serde_roundtrip() {
    let mut policy = RoutingPolicy::new_default_local();
    policy.allow_host_offload("android.gui.ISurfaceComposer");
    policy.allow_hybrid("android.gui.IGraphicBufferProducer", vec![1, 2, 3]);

    let json = serde_json::to_string(&policy).expect("Serialization failed");
    let deserialized: RoutingPolicy =
        serde_json::from_str(&json).expect("Deserialization failed");

    assert_eq!(
        deserialized.route("android.gui.ISurfaceComposer", 1006),
        RouteAction::HostOffload
    );
    assert_eq!(
        deserialized.route("android.gui.IGraphicBufferProducer", 2),
        RouteAction::HostOffload
    );
    assert_eq!(
        deserialized.route("android.gui.IGraphicBufferProducer", 4),
        RouteAction::LocalGuest
    );
}
