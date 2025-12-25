#!/usr/bin/env python3
"""Convert EasyList (AdBlock Plus format) to Safari Content Blocker JSON."""

import json
import re
import sys

def convert_rule(line):
    """Convert a single AdBlock Plus rule to Safari Content Blocker format."""
    line = line.strip()

    # Skip comments, empty lines, and exception rules
    if not line or line.startswith('!') or line.startswith('@@') or line.startswith('['):
        return None

    # Skip cosmetic rules (element hiding)
    if '##' in line or '#@#' in line or '#?#' in line:
        return None

    # Extract the pattern and options
    options = {}
    if '$' in line:
        parts = line.rsplit('$', 1)
        pattern = parts[0]
        opts = parts[1].split(',')
        for opt in opts:
            if opt.startswith('domain='):
                # Skip rules with complex domain restrictions for now
                return None
            elif opt in ('third-party', '~third-party'):
                options['third-party'] = not opt.startswith('~')
            elif opt in ('script', 'image', 'stylesheet', 'xmlhttprequest', 'subdocument', 'object'):
                options['resource-type'] = opt
    else:
        pattern = line

    # Convert pattern to regex
    url_filter = convert_pattern_to_regex(pattern)
    if not url_filter:
        return None

    # Build the rule
    trigger = {"url-filter": url_filter}

    if options.get('third-party'):
        trigger["load-type"] = ["third-party"]

    return {"trigger": trigger, "action": {"type": "block"}}

def convert_pattern_to_regex(pattern):
    """Convert AdBlock Plus pattern to regex."""
    if not pattern:
        return None

    # Handle domain rules: ||domain.com^
    if pattern.startswith('||'):
        domain = pattern[2:]
        # Remove trailing ^ or other anchors
        domain = domain.rstrip('^*')
        if not domain:
            return None
        # Escape regex special chars
        domain = re.escape(domain)
        # Convert ^ to end-of-domain marker
        return f"^https?://([^/]+\\.)?{domain}"

    # Handle start anchor: |http
    if pattern.startswith('|') and not pattern.startswith('||'):
        pattern = '^' + re.escape(pattern[1:])
    # Handle end anchor: pattern|
    elif pattern.endswith('|'):
        pattern = re.escape(pattern[:-1]) + '$'
    else:
        # Regular pattern - escape and convert wildcards
        pattern = re.escape(pattern)

    # Convert AdBlock wildcards to regex
    pattern = pattern.replace(r'\*', '.*')
    pattern = pattern.replace(r'\^', '[^a-zA-Z0-9_.%-]')

    # Skip patterns that are too generic
    if pattern in ('.*', '^.*', '.*$', ''):
        return None

    return pattern

def main():
    if len(sys.argv) < 2:
        print("Usage: convert_easylist.py <easylist.txt> [output.json]", file=sys.stderr)
        sys.exit(1)

    input_file = sys.argv[1]
    output_file = sys.argv[2] if len(sys.argv) > 2 else None

    rules = []
    skipped = 0

    with open(input_file, 'r', encoding='utf-8') as f:
        for line in f:
            rule = convert_rule(line)
            if rule:
                rules.append(rule)
            else:
                skipped += 1

    # Deduplicate rules
    seen = set()
    unique_rules = []
    for rule in rules:
        key = rule['trigger']['url-filter']
        if key not in seen:
            seen.add(key)
            unique_rules.append(rule)

    print(f"Converted {len(unique_rules)} rules (skipped {skipped})", file=sys.stderr)

    output = json.dumps(unique_rules, indent=2)

    if output_file:
        with open(output_file, 'w', encoding='utf-8') as f:
            f.write(output)
        print(f"Written to {output_file}", file=sys.stderr)
    else:
        print(output)

if __name__ == '__main__':
    main()
