//! Selective routing policy types and decision engine.

use serde::{Deserialize, Serialize};

/// Action to take for an intercepted Binder transaction.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RouteAction {
    /// Execute locally within the guest kernel /dev/binder.
    LocalGuest,
    /// Offload across Virtio-Binder boundary to the host runtime and GPU compositor.
    HostOffload,
    /// Hybrid execution: only specific transaction codes are routed to host, others stay local.
    Hybrid {
        /// Set of transaction opcodes to offload to host.
        host_codes: Vec<u32>,
    },
}

impl RouteAction {
    /// Return true if the action is purely local guest execution.
    pub fn is_local_guest(&self) -> bool {
        matches!(self, RouteAction::LocalGuest)
    }

    /// Return true if the action is unconditionally host offloaded.
    pub fn is_host_offload(&self) -> bool {
        matches!(self, RouteAction::HostOffload)
    }

    /// Return true if the action is hybrid.
    pub fn is_hybrid(&self) -> bool {
        matches!(self, RouteAction::Hybrid { .. })
    }

    /// Resolve whether a specific transaction code routes to host.
    pub fn resolves_to_host(&self, code: u32) -> bool {
        match self {
            RouteAction::HostOffload => true,
            RouteAction::Hybrid { host_codes } => host_codes.contains(&code),
            RouteAction::LocalGuest => false,
        }
    }
}

/// A single routing rule defining an action for an interface descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingRule {
    /// Interface descriptor pattern or exact string (e.g. "android.gui.ISurfaceComposer").
    pub descriptor: String,
    /// Action to take for transactions matching this descriptor.
    pub action: RouteAction,
    /// Optional service name filter (e.g. "SurfaceFlinger").
    pub service_name: Option<String>,
    /// Rule priority (higher value evaluated earlier).
    pub priority: i32,
}

impl RoutingRule {
    /// Construct a new rule with default priority 0.
    pub fn new(descriptor: impl Into<String>, action: RouteAction) -> Self {
        Self {
            descriptor: descriptor.into(),
            action,
            service_name: None,
            priority: 0,
        }
    }

    /// Attach an optional service name constraint.
    pub fn with_service_name(mut self, service: impl Into<String>) -> Self {
        self.service_name = Some(service.into());
        self
    }

    /// Set the priority for this rule.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Check if this rule matches a descriptor and optional service name.
    pub fn matches(&self, descriptor: &str, service_name: Option<&str>) -> bool {
        if let Some(expected_svc) = &self.service_name {
            match service_name {
                Some(svc) if svc == expected_svc || expected_svc == "*" => {}
                _ => return false,
            }
        }

        if self.descriptor == "*" {
            return true;
        }

        if let Some(prefix) = self.descriptor.strip_suffix('*') {
            descriptor.starts_with(prefix)
        } else if let Some(suffix) = self.descriptor.strip_prefix('*') {
            descriptor.ends_with(suffix)
        } else {
            self.descriptor == descriptor
        }
    }
}

/// Routing policy engine defaulting to local guest execution (default-deny/local).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingPolicy {
    default_action: RouteAction,
    rules: Vec<RoutingRule>,
}

impl Default for RoutingPolicy {
    fn default() -> Self {
        Self::new_default_local()
    }
}

impl RoutingPolicy {
    /// Construct a routing policy with a custom default action.
    pub fn new(default_action: RouteAction) -> Self {
        Self {
            default_action,
            rules: Vec::new(),
        }
    }

    /// Construct a standard default-deny / default-local guest policy.
    pub fn new_default_local() -> Self {
        Self::new(RouteAction::LocalGuest)
    }

    /// Add or update a rule that offloads all transactions for a descriptor to host.
    pub fn allow_host_offload(&mut self, descriptor: impl Into<String>) {
        self.set_rule(descriptor, RouteAction::HostOffload);
    }

    /// Add or update a rule that hybrid-offloads specific opcodes for a descriptor to host.
    pub fn allow_hybrid(&mut self, descriptor: impl Into<String>, host_codes: Vec<u32>) {
        self.set_rule(descriptor, RouteAction::Hybrid { host_codes });
    }

    /// Set or replace a rule for a given descriptor.
    pub fn set_rule(&mut self, descriptor: impl Into<String>, action: RouteAction) {
        let desc = descriptor.into();
        if let Some(rule) = self.rules.iter_mut().find(|r| r.descriptor == desc && r.service_name.is_none()) {
            rule.action = action;
        } else {
            self.rules.push(RoutingRule::new(desc, action));
            self.sort_rules();
        }
    }

    /// Add a custom `RoutingRule` into the policy.
    pub fn add_rule(&mut self, rule: RoutingRule) {
        self.rules.push(rule);
        self.sort_rules();
    }

    /// Remove an existing rule by exact descriptor match.
    pub fn remove_rule(&mut self, descriptor: &str) -> Option<RoutingRule> {
        if let Some(pos) = self.rules.iter().position(|r| r.descriptor == descriptor && r.service_name.is_none()) {
            Some(self.rules.remove(pos))
        } else {
            None
        }
    }

    /// Determine the `RouteAction` for a given descriptor and transaction code.
    pub fn route(&self, descriptor: &str, code: u32) -> RouteAction {
        self.route_service(None, descriptor, code)
    }

    /// Determine the `RouteAction` for an optional service name, descriptor, and transaction code.
    pub fn route_service(&self, service_name: Option<&str>, descriptor: &str, code: u32) -> RouteAction {
        for rule in &self.rules {
            if rule.matches(descriptor, service_name) {
                return match &rule.action {
                    RouteAction::HostOffload => RouteAction::HostOffload,
                    RouteAction::LocalGuest => RouteAction::LocalGuest,
                    RouteAction::Hybrid { host_codes } => {
                        if host_codes.contains(&code) {
                            RouteAction::HostOffload
                        } else {
                            RouteAction::LocalGuest
                        }
                    }
                };
            }
        }
        self.default_action.clone()
    }

    /// Return reference to the default route action.
    pub fn default_action(&self) -> &RouteAction {
        &self.default_action
    }

    /// Set the default route action.
    pub fn set_default_action(&mut self, action: RouteAction) {
        self.default_action = action;
    }

    /// Return all configured rules.
    pub fn rules(&self) -> &[RoutingRule] {
        &self.rules
    }

    /// Return count of configured rules.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Return true if no custom rules exist.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Clear all rules.
    pub fn clear(&mut self) {
        self.rules.clear();
    }

    fn sort_rules(&mut self) {
        // Sort in descending order of priority
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }
}
