//! Pattern matchers for Binder interface descriptors, transaction opcodes, and service names.

use crate::policy::RouteAction;
use serde::{Deserialize, Serialize};

/// Pattern matcher for interface descriptors (e.g. "android.gui.ISurfaceComposer").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DescriptorMatcher {
    /// Match everything.
    Any,
    /// Match exact string.
    Exact(String),
    /// Match by prefix (e.g. "android.gui.").
    Prefix(String),
    /// Match by suffix (e.g. "ISurfaceComposer").
    Suffix(String),
    /// Wildcard glob pattern (supports `*` anywhere).
    Wildcard(String),
}

impl DescriptorMatcher {
    /// Check whether a descriptor candidate matches this pattern.
    pub fn matches(&self, candidate: &str) -> bool {
        match self {
            DescriptorMatcher::Any => true,
            DescriptorMatcher::Exact(s) => candidate == s,
            DescriptorMatcher::Prefix(prefix) => candidate.starts_with(prefix),
            DescriptorMatcher::Suffix(suffix) => candidate.ends_with(suffix),
            DescriptorMatcher::Wildcard(pattern) => wildcard_match(pattern, candidate),
        }
    }
}

/// Filter criteria for Binder transaction opcodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodeFilter {
    /// Match all transaction codes.
    All,
    /// Match specific whitelist of opcodes.
    Specific(Vec<u32>),
    /// Match an inclusive numeric range of opcodes `[min, max]`.
    Range(u32, u32),
    /// Match all opcodes EXCEPT the specified blacklist.
    Except(Vec<u32>),
}

impl CodeFilter {
    /// Check whether a transaction code matches this filter.
    pub fn matches(&self, code: u32) -> bool {
        match self {
            CodeFilter::All => true,
            CodeFilter::Specific(codes) => codes.contains(&code),
            CodeFilter::Range(min, max) => code >= *min && code <= *max,
            CodeFilter::Except(codes) => !codes.contains(&code),
        }
    }
}

/// Pattern matcher for Android service names (e.g. "SurfaceFlinger", "activity").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceNameMatcher {
    /// Match any service name (or when service name is not specified).
    Any,
    /// Match exact service name.
    Exact(String),
    /// Match prefix.
    Prefix(String),
    /// Wildcard glob pattern.
    Wildcard(String),
}

impl ServiceNameMatcher {
    /// Check whether a service name matches this pattern.
    pub fn matches(&self, candidate: Option<&str>) -> bool {
        match (self, candidate) {
            (ServiceNameMatcher::Any, _) => true,
            (ServiceNameMatcher::Exact(expected), Some(actual)) => expected == actual,
            (ServiceNameMatcher::Prefix(prefix), Some(actual)) => actual.starts_with(prefix),
            (ServiceNameMatcher::Wildcard(pattern), Some(actual)) => wildcard_match(pattern, actual),
            _ => false,
        }
    }
}

/// Comprehensive match rule combining descriptor, service name, opcode filter, and priority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchRule {
    /// Interface descriptor pattern.
    pub descriptor: DescriptorMatcher,
    /// Service name pattern.
    pub service: ServiceNameMatcher,
    /// Transaction code filter.
    pub code: CodeFilter,
    /// Resulting route action if rule matches.
    pub action: RouteAction,
    /// Priority for evaluation order (higher evaluated first).
    pub priority: i32,
    /// Human-readable description.
    pub description: Option<String>,
}

impl MatchRule {
    /// Construct a new match rule with default priority.
    pub fn new(descriptor: DescriptorMatcher, action: RouteAction) -> Self {
        Self {
            descriptor,
            service: ServiceNameMatcher::Any,
            code: CodeFilter::All,
            action,
            priority: 0,
            description: None,
        }
    }

    /// Add a service name matcher constraint.
    pub fn with_service(mut self, service: ServiceNameMatcher) -> Self {
        self.service = service;
        self
    }

    /// Add a transaction code filter constraint.
    pub fn with_code_filter(mut self, code: CodeFilter) -> Self {
        self.code = code;
        self
    }

    /// Set rule priority.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Check if this rule matches the transaction parameters.
    pub fn matches(&self, service_name: Option<&str>, descriptor: Option<&str>, code: u32) -> bool {
        if !self.service.matches(service_name) {
            return false;
        }

        if let Some(desc) = descriptor {
            if !self.descriptor.matches(desc) {
                return false;
            }
        } else if !matches!(self.descriptor, DescriptorMatcher::Any) {
            return false;
        }

        self.code.matches(code)
    }
}

/// Advanced matcher engine evaluating structured `MatchRule` sets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatcherEngine {
    default_action: RouteAction,
    rules: Vec<MatchRule>,
}

impl Default for MatcherEngine {
    fn default() -> Self {
        Self::new(RouteAction::LocalGuest)
    }
}

impl MatcherEngine {
    /// Construct a new engine with specified default action.
    pub fn new(default_action: RouteAction) -> Self {
        Self {
            default_action,
            rules: Vec::new(),
        }
    }

    /// Add a rule into the engine, maintaining priority ordering.
    pub fn add_rule(&mut self, rule: MatchRule) {
        self.rules.push(rule);
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Evaluate transaction parameters against the rule set.
    pub fn match_transaction(
        &self,
        service_name: Option<&str>,
        descriptor: Option<&str>,
        code: u32,
    ) -> RouteAction {
        for rule in &self.rules {
            if rule.matches(service_name, descriptor, code) {
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

    /// Return count of rules.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Return true if no rules exist.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Clear all rules.
    pub fn clear(&mut self) {
        self.rules.clear();
    }
}

/// Standard glob wildcard matching helper supporting `*`.
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p_chars: Vec<char> = pattern.chars().collect();
    let t_chars: Vec<char> = text.chars().collect();

    let mut p_idx = 0;
    let mut t_idx = 0;
    let mut star_idx = None;
    let mut match_idx = 0;

    while t_idx < t_chars.len() {
        if p_idx < p_chars.len() && p_chars[p_idx] == t_chars[t_idx] {
            p_idx += 1;
            t_idx += 1;
        } else if p_idx < p_chars.len() && p_chars[p_idx] == '*' {
            star_idx = Some(p_idx);
            match_idx = t_idx;
            p_idx += 1;
        } else if let Some(star) = star_idx {
            p_idx = star + 1;
            match_idx += 1;
            t_idx = match_idx;
        } else {
            return false;
        }
    }

    while p_idx < p_chars.len() && p_chars[p_idx] == '*' {
        p_idx += 1;
    }

    p_idx == p_chars.len()
}
