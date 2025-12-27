//! Built-in scriptlet implementations
//!
//! Each scriptlet generates JavaScript code that gets injected into matching pages.
//! Based on uBlock Origin's scriptlets.

use super::parser::ScriptletRule;

/// Generate JavaScript code for a scriptlet rule
pub fn generate_script(rule: &ScriptletRule) -> Option<String> {
    let js = match rule.scriptlet_name.as_str() {
        "set-constant" | "set" => generate_set_constant(&rule.args),
        "json-prune" => generate_json_prune(&rule.args),
        "prune-fetch-response" => generate_prune_fetch_response(&rule.args),
        "prune-property-on-set" => generate_prune_property_on_set(&rule.args),
        "abort-on-property-read" => generate_abort_on_property_read(&rule.args),
        "abort-on-property-write" => generate_abort_on_property_write(&rule.args),
        "addEventListener-defuser" => generate_aeld(&rule.args),
        "no-setTimeout-if" => generate_no_settimeout_if(&rule.args),
        "no-setInterval-if" => generate_no_setinterval_if(&rule.args),
        "prevent-fetch" | "no-fetch-if" => generate_prevent_fetch(&rule.args),
        "prevent-xhr" | "no-xhr-if" => generate_prevent_xhr(&rule.args),
        _ => {
            log::debug!("Unknown scriptlet: {}", rule.scriptlet_name);
            return None;
        }
    }?;

    // Wrap in IIFE with error handling
    Some(format!(
        r#"(function() {{
    'use strict';
    try {{
        {}
    }} catch (e) {{
        // Silent fail
    }}
}})();"#,
        js
    ))
}

/// set-constant: Override a property with a constant value
/// Args: [property_path, value]
fn generate_set_constant(args: &[String]) -> Option<String> {
    let property = args.get(0)?;
    let value = args.get(1).map(|s| s.as_str()).unwrap_or("undefined");

    // Convert value string to JS literal
    let js_value = match value {
        "undefined" => "undefined",
        "null" => "null",
        "true" => "true",
        "false" => "false",
        "noopFunc" => "(function(){})",
        "trueFunc" => "(function(){return true})",
        "falseFunc" => "(function(){return false})",
        "emptyStr" => "''",
        "emptyArr" | "[]" => "[]",
        "emptyObj" | "{}" => "{}",
        s if s.parse::<f64>().is_ok() => s,
        s => return Some(format!(r#"/* unsupported value: {} */"#, s)),
    };

    Some(format!(
        r#"
        const chain = '{}';
        const cValue = {};

        const setConstant = function(chain, cValue) {{
            const props = chain.split('.');
            let owner = window;

            for (let i = 0; i < props.length - 1; i++) {{
                const prop = props[i];
                if (!(prop in owner)) {{
                    Object.defineProperty(owner, prop, {{
                        value: {{}},
                        writable: true,
                        enumerable: true,
                        configurable: true
                    }});
                }}
                owner = owner[prop];
                if (owner === null || (typeof owner !== 'object' && typeof owner !== 'function')) {{
                    return;
                }}
            }}

            const lastProp = props[props.length - 1];
            try {{
                Object.defineProperty(owner, lastProp, {{
                    get: function() {{ return cValue; }},
                    set: function() {{}},
                    enumerable: true,
                    configurable: false
                }});
            }} catch (e) {{
                owner[lastProp] = cValue;
            }}
        }};

        setConstant(chain, cValue);
"#,
        property, js_value
    ))
}

/// json-prune: Remove properties from JSON.parse results
/// Args: [properties_to_remove, optional_needle]
fn generate_json_prune(args: &[String]) -> Option<String> {
    let props = args.get(0)?;
    let needle = args.get(1).map(|s| s.as_str()).unwrap_or("");

    // Optimized: only prune if text contains a needle (fast string check)
    Some(format!(
        r#"
        const propsToRemove = '{}';
        const needle = '{}';

        const pruner = function(obj, props) {{
            if (typeof obj !== 'object' || obj === null) return obj;
            const propList = props.split(' ');
            for (const propPath of propList) {{
                if (!propPath) continue;
                const parts = propPath.split('.');
                let current = obj;
                for (let i = 0; i < parts.length - 1; i++) {{
                    if (!current || typeof current !== 'object') break;
                    current = current[parts[i]];
                }}
                if (current && typeof current === 'object') {{
                    delete current[parts[parts.length - 1]];
                }}
            }}
            return obj;
        }};

        const origParse = JSON.parse;
        JSON.parse = function(text, reviver) {{
            // Fast path: skip if needle not found in raw text
            if (needle && typeof text === 'string' && !text.includes(needle)) {{
                return origParse.call(this, text, reviver);
            }}
            const result = origParse.call(this, text, reviver);
            return pruner(result, propsToRemove);
        }};
"#,
        props, needle
    ))
}

/// abort-on-property-read: Throw when a property is accessed
/// Args: [property_path]
fn generate_abort_on_property_read(args: &[String]) -> Option<String> {
    let property = args.get(0)?;

    Some(format!(
        r#"
        const chain = '{}';
        const props = chain.split('.');
        let owner = window;

        for (let i = 0; i < props.length - 1; i++) {{
            if (!(props[i] in owner)) {{
                owner[props[i]] = {{}};
            }}
            owner = owner[props[i]];
        }}

        const lastProp = props[props.length - 1];
        Object.defineProperty(owner, lastProp, {{
            get: function() {{
                throw new ReferenceError('aopr: ' + chain);
            }},
            set: function() {{}},
            configurable: true
        }});
"#,
        property
    ))
}

/// abort-on-property-write: Throw when a property is written
/// Args: [property_path]
fn generate_abort_on_property_write(args: &[String]) -> Option<String> {
    let property = args.get(0)?;

    Some(format!(
        r#"
        const chain = '{}';
        const props = chain.split('.');
        let owner = window;

        for (let i = 0; i < props.length - 1; i++) {{
            if (!(props[i] in owner)) {{
                owner[props[i]] = {{}};
            }}
            owner = owner[props[i]];
        }}

        const lastProp = props[props.length - 1];
        let currentValue = owner[lastProp];
        Object.defineProperty(owner, lastProp, {{
            get: function() {{ return currentValue; }},
            set: function(value) {{
                throw new ReferenceError('aopw: ' + chain);
            }},
            configurable: true
        }});
"#,
        property
    ))
}

/// addEventListener-defuser: Prevent specific event listeners
/// Args: [type_pattern, handler_pattern]
fn generate_aeld(args: &[String]) -> Option<String> {
    let type_pattern = args.get(0).map(|s| s.as_str()).unwrap_or("");
    let handler_pattern = args.get(1).map(|s| s.as_str()).unwrap_or("");

    Some(format!(
        r#"
        const typePattern = '{}';
        const handlerPattern = '{}';

        const origAddEventListener = EventTarget.prototype.addEventListener;
        EventTarget.prototype.addEventListener = function(type, handler, options) {{
            if (typePattern && !type.includes(typePattern)) {{
                return origAddEventListener.call(this, type, handler, options);
            }}
            if (handlerPattern && handler && !handler.toString().includes(handlerPattern)) {{
                return origAddEventListener.call(this, type, handler, options);
            }}
            // Blocked
        }};
"#,
        type_pattern, handler_pattern
    ))
}

/// no-setTimeout-if: Block setTimeout calls matching a pattern
/// Args: [pattern, optional_delay]
fn generate_no_settimeout_if(args: &[String]) -> Option<String> {
    let pattern = args.get(0).map(|s| s.as_str()).unwrap_or("");

    Some(format!(
        r#"
        const pattern = '{}';
        const origSetTimeout = window.setTimeout;

        window.setTimeout = function(fn, delay, ...args) {{
            const fnStr = typeof fn === 'function' ? fn.toString() : String(fn);
            if (pattern && fnStr.includes(pattern)) {{
                return 0; // Blocked
            }}
            return origSetTimeout.call(this, fn, delay, ...args);
        }};
"#,
        pattern
    ))
}

/// no-setInterval-if: Block setInterval calls matching a pattern
/// Args: [pattern, optional_delay]
fn generate_no_setinterval_if(args: &[String]) -> Option<String> {
    let pattern = args.get(0).map(|s| s.as_str()).unwrap_or("");

    Some(format!(
        r#"
        const pattern = '{}';
        const origSetInterval = window.setInterval;

        window.setInterval = function(fn, delay, ...args) {{
            const fnStr = typeof fn === 'function' ? fn.toString() : String(fn);
            if (pattern && fnStr.includes(pattern)) {{
                return 0; // Blocked
            }}
            return origSetInterval.call(this, fn, delay, ...args);
        }};
"#,
        pattern
    ))
}

/// prevent-fetch: Block fetch calls matching a URL pattern
/// Args: [url_pattern]
fn generate_prevent_fetch(args: &[String]) -> Option<String> {
    let pattern = args.get(0).map(|s| s.as_str()).unwrap_or("");

    Some(format!(
        r#"
        const pattern = '{}';
        const origFetch = window.fetch;

        window.fetch = function(resource, options) {{
            const url = typeof resource === 'string' ? resource :
                        resource instanceof Request ? resource.url : String(resource);
            if (pattern && url.includes(pattern)) {{
                return Promise.reject(new TypeError('Fetch blocked'));
            }}
            return origFetch.call(this, resource, options);
        }};
"#,
        pattern
    ))
}

/// prune-property-on-set: Intercept when a window property is SET and prune sub-properties
/// Args: [property_name, properties_to_remove]
/// E.g., prune-property-on-set(ytInitialPlayerResponse, adPlacements playerAds adSlots)
fn generate_prune_property_on_set(args: &[String]) -> Option<String> {
    let prop_name = args.get(0)?;
    let props_to_remove = args.get(1).map(|s| s.as_str()).unwrap_or("adPlacements playerAds adSlots");

    Some(format!(
        r#"
        const propName = '{}';
        const propsToRemove = '{}';

        const deepPrune = function(obj, props) {{
            if (!obj || typeof obj !== 'object') return;
            const propList = props.split(/\s+/);

            for (const key of Object.keys(obj)) {{
                if (propList.includes(key)) {{
                    delete obj[key];
                }} else if (typeof obj[key] === 'object' && obj[key] !== null) {{
                    deepPrune(obj[key], props);
                }}
            }}
        }};

        let storedValue = window[propName];

        Object.defineProperty(window, propName, {{
            get: function() {{
                return storedValue;
            }},
            set: function(value) {{
                if (value && typeof value === 'object') {{
                    deepPrune(value, propsToRemove);
                }}
                storedValue = value;
            }},
            configurable: true,
            enumerable: true
        }});
"#,
        prop_name, props_to_remove
    ))
}

/// prune-fetch-response: Intercept fetch responses and remove ad-related JSON properties
/// Args: [properties_to_remove]
/// Simpler implementation that prunes common ad properties from ALL JSON responses
fn generate_prune_fetch_response(args: &[String]) -> Option<String> {
    let props = args.get(0).map(|s| s.as_str()).unwrap_or("adPlacements playerAds adSlots");

    Some(format!(
        r#"
        const propsToRemove = '{}';

        const deepPrune = function(obj, props) {{
            if (!obj || typeof obj !== 'object') return;
            const propList = props.split(/\s+/);

            for (const key of Object.keys(obj)) {{
                if (propList.includes(key)) {{
                    delete obj[key];
                }} else if (typeof obj[key] === 'object') {{
                    deepPrune(obj[key], props);
                }}
            }}
        }};

        const origFetch = window.fetch;
        window.fetch = async function(resource, options) {{
            const response = await origFetch.call(this, resource, options);

            const url = typeof resource === 'string' ? resource :
                        resource instanceof Request ? resource.url : String(resource);

            // Only process YouTube API calls
            if (!url.includes('/youtubei/') && !url.includes('/player')) {{
                return response;
            }}

            const contentType = response.headers.get('content-type') || '';
            if (!contentType.includes('json')) {{
                return response;
            }}

            try {{
                const clone = response.clone();
                const text = await clone.text();
                const data = JSON.parse(text);

                deepPrune(data, propsToRemove);

                return new Response(JSON.stringify(data), {{
                    status: response.status,
                    statusText: response.statusText,
                    headers: response.headers
                }});
            }} catch (e) {{
                return response;
            }}
        }};
"#,
        props
    ))
}

/// prevent-xhr: Block XMLHttpRequest calls matching a URL pattern
/// Args: [url_pattern]
fn generate_prevent_xhr(args: &[String]) -> Option<String> {
    let pattern = args.get(0).map(|s| s.as_str()).unwrap_or("");

    Some(format!(
        r#"
        const pattern = '{}';
        const origOpen = XMLHttpRequest.prototype.open;

        XMLHttpRequest.prototype.open = function(method, url, ...args) {{
            if (pattern && url.includes(pattern)) {{
                this._blocked = true;
            }}
            return origOpen.call(this, method, url, ...args);
        }};

        const origSend = XMLHttpRequest.prototype.send;
        XMLHttpRequest.prototype.send = function(body) {{
            if (this._blocked) {{
                return;
            }}
            return origSend.call(this, body);
        }};
"#,
        pattern
    ))
}
