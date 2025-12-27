//! Parser for uBlock Origin filter syntax
//!
//! Parses filters like:
//! `youtube.com##+js(set-constant, ytInitialPlayerResponse.playerAds, undefined)`

/// A parsed scriptlet rule
#[derive(Debug, Clone)]
pub struct ScriptletRule {
    /// Domain patterns (e.g., ["youtube.com", "m.youtube.com"])
    pub domains: Vec<String>,
    /// Scriptlet name (e.g., "set-constant")
    pub scriptlet_name: String,
    /// Arguments (e.g., ["ytInitialPlayerResponse.playerAds", "undefined"])
    pub args: Vec<String>,
}

impl ScriptletRule {
    /// Parse a filter line like "youtube.com##+js(set-constant, foo, bar)"
    ///
    /// Returns None if the line is not a valid scriptlet rule.
    pub fn parse(line: &str) -> Option<Self> {
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('!') || line.starts_with('#') {
            return None;
        }

        // Find the ##+js( marker
        let js_marker = "##+js(";
        let marker_pos = line.find(js_marker)?;

        // Extract domains (everything before the marker)
        let domains_str = &line[..marker_pos];
        let domains = Self::parse_domains(domains_str);

        if domains.is_empty() {
            return None;
        }

        // Extract scriptlet call (everything after ##+js( until closing paren)
        let call_start = marker_pos + js_marker.len();
        let call_str = &line[call_start..];

        // Find the closing paren (handle nested parens if needed)
        let call_end = Self::find_closing_paren(call_str)?;
        let call_content = &call_str[..call_end];

        // Parse scriptlet name and arguments
        let (scriptlet_name, args) = Self::parse_call(call_content)?;

        Some(ScriptletRule {
            domains,
            scriptlet_name,
            args,
        })
    }

    /// Parse domain list, handling commas and negation
    fn parse_domains(s: &str) -> Vec<String> {
        s.split(',')
            .map(|d| d.trim())
            .filter(|d| !d.is_empty() && !d.starts_with('~')) // Skip negated domains for now
            .map(|d| d.to_string())
            .collect()
    }

    /// Find the position of the closing parenthesis
    fn find_closing_paren(s: &str) -> Option<usize> {
        let mut depth = 0;
        let mut in_string = false;
        let mut escape_next = false;

        for (i, c) in s.char_indices() {
            if escape_next {
                escape_next = false;
                continue;
            }

            match c {
                '\\' => escape_next = true,
                '"' | '\'' => in_string = !in_string,
                '(' if !in_string => depth += 1,
                ')' if !in_string => {
                    if depth == 0 {
                        return Some(i);
                    }
                    depth -= 1;
                }
                _ => {}
            }
        }
        None
    }

    /// Parse the scriptlet call content into name and arguments
    fn parse_call(content: &str) -> Option<(String, Vec<String>)> {
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut in_string = false;
        let mut escape_next = false;

        for c in content.chars() {
            if escape_next {
                current.push(c);
                escape_next = false;
                continue;
            }

            match c {
                '\\' => escape_next = true,
                ',' if !in_string => {
                    parts.push(current.trim().to_string());
                    current = String::new();
                }
                '"' | '\'' => in_string = !in_string,
                _ => current.push(c),
            }
        }

        // Don't forget the last part
        let last = current.trim().to_string();
        if !last.is_empty() {
            parts.push(last);
        }

        if parts.is_empty() {
            return None;
        }

        let scriptlet_name = Self::normalize_scriptlet_name(&parts[0]);
        let args = parts.into_iter().skip(1).collect();

        Some((scriptlet_name, args))
    }

    /// Normalize scriptlet name aliases
    fn normalize_scriptlet_name(name: &str) -> String {
        match name.trim() {
            // Common aliases
            "set" => "set-constant".to_string(),
            "aopr" => "abort-on-property-read".to_string(),
            "aopw" => "abort-on-property-write".to_string(),
            "aeld" => "addEventListener-defuser".to_string(),
            "aell" => "addEventListener-logger".to_string(),
            "nano-sib" | "nano-setInterval-booster" => "nano-setInterval-booster".to_string(),
            "nano-stb" | "nano-setTimeout-booster" => "nano-setTimeout-booster".to_string(),
            "nosiif" | "no-setInterval-if" => "no-setInterval-if".to_string(),
            "nostif" | "no-setTimeout-if" => "no-setTimeout-if".to_string(),
            "nowoif" | "no-window-open-if" => "no-window-open-if".to_string(),
            "ra" => "remove-attr".to_string(),
            "rc" => "remove-class".to_string(),
            other => other.to_string(),
        }
    }

    /// Generate WebKit allow_list patterns for this rule
    /// E.g., ["*://*.youtube.com/*", "*://youtube.com/*"]
    pub fn webkit_allow_list(&self) -> Vec<String> {
        self.domains
            .iter()
            .flat_map(|domain| {
                vec![
                    format!("*://{}/*", domain),
                    format!("*://*.{}/*", domain),
                ]
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_rule() {
        let rule = ScriptletRule::parse(
            "youtube.com##+js(set-constant, ytInitialPlayerResponse.playerAds, undefined)",
        )
        .unwrap();

        assert_eq!(rule.domains, vec!["youtube.com"]);
        assert_eq!(rule.scriptlet_name, "set-constant");
        assert_eq!(
            rule.args,
            vec!["ytInitialPlayerResponse.playerAds", "undefined"]
        );
    }

    #[test]
    fn test_parse_multiple_domains() {
        let rule = ScriptletRule::parse(
            "youtube.com,m.youtube.com##+js(set, foo, bar)",
        )
        .unwrap();

        assert_eq!(rule.domains, vec!["youtube.com", "m.youtube.com"]);
        assert_eq!(rule.scriptlet_name, "set-constant"); // normalized from "set"
    }

    #[test]
    fn test_parse_alias() {
        let rule = ScriptletRule::parse("example.com##+js(aopr, someProperty)").unwrap();
        assert_eq!(rule.scriptlet_name, "abort-on-property-read");
    }

    #[test]
    fn test_skip_comment() {
        assert!(ScriptletRule::parse("! This is a comment").is_none());
        assert!(ScriptletRule::parse("# Another comment").is_none());
    }

    #[test]
    fn test_webkit_allow_list() {
        let rule = ScriptletRule::parse("youtube.com##+js(set, foo, bar)").unwrap();
        let patterns = rule.webkit_allow_list();

        assert!(patterns.contains(&"*://youtube.com/*".to_string()));
        assert!(patterns.contains(&"*://*.youtube.com/*".to_string()));
    }
}
