// Parse Linux `tc` (traffic control) output and print a human-readable
// summary of netem impairment state and traffic statistics. Replaces the
// awk script previously embedded in scripts/netem-lib.sh.
//
// Input is produced by running this inside the netem container:
//   for iface in $(ls /sys/class/net/); do
//       echo "===IFACE $iface"
//       tc -s qdisc show dev "$iface" 2>/dev/null
//       echo "---FILTERS---"
//       tc -s filter show dev "$iface" 2>/dev/null
//   done

use std::io::{self, Read};

use anyhow::Result;
use clap::Parser;

/// Parse tc output from a netem container and print a human-readable digest.
#[derive(Parser)]
#[command(name = "netem-digest")]
struct Cli {}

fn main() -> Result<()> {
    let _cli = Cli::parse();
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let output = digest(&input);
    print!("{output}");
    Ok(())
}

/// Convert an 8-digit hex string to a dotted-decimal IP address.
fn hex_to_ip(hex: &str) -> String {
    if hex.len() != 8 {
        return "unknown".to_string();
    }
    let mut octets = Vec::with_capacity(4);
    for i in 0..4 {
        let byte_str = &hex[i * 2..i * 2 + 2];
        let val = u8::from_str_radix(byte_str, 16).unwrap_or(0);
        octets.push(val.to_string());
    }
    octets.join(".")
}

/// Format a byte count as a human-readable string (B, KB, MB, GB, TB).
fn fmt_bytes(n: u64) -> String {
    if n == 0 {
        return "0 B".to_string();
    }
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut val = n as f64;
    let mut idx = 0;
    while val >= 1024.0 && idx < UNITS.len() - 1 {
        val /= 1024.0;
        idx += 1;
    }
    if idx == 0 {
        format!("{n} {}", UNITS[0])
    } else {
        format!("{val:.1} {}", UNITS[idx])
    }
}

/// Traffic statistics for a netem qdisc.
#[derive(Default, Clone)]
struct Stats {
    sent_bytes: u64,
    sent_pkts: u64,
    dropped: u64,
}

impl Stats {
    fn has_traffic(&self) -> bool {
        self.sent_bytes > 0 || self.sent_pkts > 0 || self.dropped > 0
    }
}

/// A netem qdisc leaf with its parameters and traffic stats.
#[derive(Clone)]
struct NetemLeaf {
    handle: String,
    params: String,
    stats: Stats,
}

/// Parsed state for a single network interface.
struct IfaceState {
    name: String,
    is_flat: bool,
    has_netem: bool,
    default_class: String,
    leaves: Vec<NetemLeaf>,
    /// Map from class ID to destination IP (from u32 filters).
    filter_ips: Vec<(String, String)>,
}

impl IfaceState {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            is_flat: false,
            has_netem: false,
            default_class: String::new(),
            leaves: Vec::new(),
            filter_ips: Vec::new(),
        }
    }

    fn filter_ip_for(&self, handle: &str) -> Option<&str> {
        self.filter_ips
            .iter()
            .find(|(h, _)| h == handle)
            .map(|(_, ip)| ip.as_str())
    }

    /// Format this interface's digest output, matching the awk script behavior.
    fn format(&self) -> String {
        if !self.has_netem {
            return String::new();
        }

        let any_traffic = self.leaves.iter().any(|l| l.stats.has_traffic());
        if !any_traffic {
            return format!("{}: no traffic\n", self.name);
        }

        if self.is_flat {
            let leaf = &self.leaves[0];
            return format!(
                "{}:\n  impairment: {}\n  traffic:    {} sent ({} packets), {} dropped\n",
                self.name,
                leaf.params,
                fmt_bytes(leaf.stats.sent_bytes),
                leaf.stats.sent_pkts,
                leaf.stats.dropped,
            );
        }

        let mut out = format!("{}: per-link\n", self.name);

        // Named links (non-default classes with traffic).
        for leaf in &self.leaves {
            if leaf.handle == self.default_class || !leaf.stats.has_traffic() {
                continue;
            }
            let dst = self.filter_ip_for(&leaf.handle).unwrap_or("unknown");
            out.push_str(&format!("  link 1:{} -> {}\n", leaf.handle, dst));
            out.push_str(&format!("    impairment: {}\n", leaf.params));
            out.push_str(&format!(
                "    traffic:    {} sent ({} packets), {} dropped\n",
                fmt_bytes(leaf.stats.sent_bytes),
                leaf.stats.sent_pkts,
                leaf.stats.dropped,
            ));
        }

        // Default class (shown last, only if it has traffic).
        if let Some(default_leaf) = self.leaves.iter().find(|l| l.handle == self.default_class) {
            if !default_leaf.params.is_empty() && default_leaf.stats.has_traffic() {
                out.push_str(&format!("  default (1:{})\n", self.default_class));
                out.push_str(&format!("    impairment: {}\n", default_leaf.params));
                out.push_str(&format!(
                    "    traffic:    {} sent ({} packets), {} dropped\n",
                    fmt_bytes(default_leaf.stats.sent_bytes),
                    default_leaf.stats.sent_pkts,
                    default_leaf.stats.dropped,
                ));
            }
        }

        out
    }
}

/// Parse the full tc dump and produce a human-readable digest.
fn digest(input: &str) -> String {
    let mut output = String::new();
    let mut iface: Option<IfaceState> = None;

    #[derive(PartialEq)]
    enum Section {
        Qdisc,
        Filter,
    }
    let mut section = Section::Qdisc;
    let mut current_handle = String::new();
    let mut is_netem = false;
    let mut current_filter_class = String::new();

    for line in input.lines() {
        // New interface boundary.
        if let Some(name) = line.strip_prefix("===IFACE ") {
            if let Some(prev) = iface.take() {
                output.push_str(&prev.format());
            }
            iface = Some(IfaceState::new(name.trim()));
            section = Section::Qdisc;
            is_netem = false;
            continue;
        }

        if line == "---FILTERS---" {
            section = Section::Filter;
            continue;
        }

        let Some(state) = iface.as_mut() else {
            continue;
        };

        if section == Section::Qdisc {
            if line.starts_with("qdisc htb") {
                is_netem = false;
                // Extract default class from "default <hex>".
                let fields: Vec<&str> = line.split_whitespace().collect();
                for (i, &f) in fields.iter().enumerate() {
                    if f == "default" {
                        if let Some(&val) = fields.get(i + 1) {
                            state.default_class = val.strip_prefix("0x").unwrap_or(val).to_string();
                        }
                    }
                }
                continue;
            }

            if line.starts_with("qdisc netem") {
                let fields: Vec<&str> = line.split_whitespace().collect();
                // Handle is field[2], strip trailing colon.
                current_handle = fields
                    .get(2)
                    .unwrap_or(&"")
                    .trim_end_matches(':')
                    .to_string();
                is_netem = true;
                state.has_netem = true;

                // Check if flat (root netem, not under HTB).
                if fields.get(3) == Some(&"root") {
                    state.is_flat = true;
                }

                // Extract params: everything after "limit <N>".
                let mut params = String::new();
                let mut past_limit = false;
                let mut skip_next = false;
                for (i, &f) in fields.iter().enumerate() {
                    if skip_next {
                        skip_next = false;
                        continue;
                    }
                    if f == "limit" {
                        if let Some(next) = fields.get(i + 1) {
                            if next.chars().all(|c| c.is_ascii_digit()) {
                                past_limit = true;
                                skip_next = true;
                                continue;
                            }
                        }
                    }
                    if past_limit {
                        if params.is_empty() {
                            params = f.to_string();
                        } else {
                            params.push(' ');
                            params.push_str(f);
                        }
                    }
                }
                let params = if params.is_empty() {
                    "(no impairment)".to_string()
                } else {
                    params
                };

                state.leaves.push(NetemLeaf {
                    handle: current_handle.clone(),
                    params,
                    stats: Stats::default(),
                });
                continue;
            }

            // Stats line for the current netem qdisc.
            if is_netem && line.contains("Sent") {
                let fields: Vec<&str> = line.split_whitespace().collect();
                let mut sent_bytes = 0u64;
                let mut sent_pkts = 0u64;
                let mut dropped = 0u64;

                for (i, &f) in fields.iter().enumerate() {
                    if f == "Sent" {
                        if let Some(&val) = fields.get(i + 1) {
                            sent_bytes = val.parse().unwrap_or(0);
                        }
                    }
                    if f == "pkt" {
                        if let Some(&val) = fields.get(i.wrapping_sub(1)) {
                            sent_pkts = val.parse().unwrap_or(0);
                        }
                    }
                    if f == "(dropped" {
                        if let Some(&val) = fields.get(i + 1) {
                            let cleaned = val.trim_end_matches(',');
                            dropped = cleaned.parse().unwrap_or(0);
                        }
                    }
                }

                if let Some(leaf) = state.leaves.iter_mut().find(|l| l.handle == current_handle) {
                    leaf.stats = Stats {
                        sent_bytes,
                        sent_pkts,
                        dropped,
                    };
                }
                is_netem = false;
                continue;
            }

            // Any other qdisc line resets the netem flag.
            if line.starts_with("qdisc") {
                is_netem = false;
            }
        }

        if section == Section::Filter {
            // Extract flowid class.
            if line.contains("flowid") {
                let fields: Vec<&str> = line.split_whitespace().collect();
                for (i, &f) in fields.iter().enumerate() {
                    if f == "flowid" {
                        if let Some(&val) = fields.get(i + 1) {
                            current_filter_class =
                                val.strip_prefix("1:").unwrap_or(val).to_string();
                        }
                    }
                }
                continue;
            }

            // Extract destination IP from u32 match at offset 16.
            if line.contains("match") && line.contains("at 16") {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if let Some(&match_val) = fields.get(1) {
                    let hex_part = match_val.split('/').next().unwrap_or("");
                    let ip = hex_to_ip(hex_part);
                    state.filter_ips.push((current_filter_class.clone(), ip));
                }
                continue;
            }
        }
    }

    // Flush the last interface.
    if let Some(prev) = iface.take() {
        output.push_str(&prev.format());
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_to_ip() {
        assert_eq!(hex_to_ip("0a63000a"), "10.99.0.10");
        assert_eq!(hex_to_ip("0a630014"), "10.99.0.20");
        assert_eq!(hex_to_ip("c0a80001"), "192.168.0.1");
        assert_eq!(hex_to_ip("7f000001"), "127.0.0.1");
        assert_eq!(hex_to_ip("00000000"), "0.0.0.0");
        assert_eq!(hex_to_ip("ffffffff"), "255.255.255.255");
    }

    #[test]
    fn test_hex_to_ip_invalid() {
        assert_eq!(hex_to_ip("short"), "unknown");
        assert_eq!(hex_to_ip(""), "unknown");
    }

    #[test]
    fn test_fmt_bytes() {
        assert_eq!(fmt_bytes(0), "0 B");
        assert_eq!(fmt_bytes(100), "100 B");
        assert_eq!(fmt_bytes(1023), "1023 B");
        assert_eq!(fmt_bytes(1024), "1.0 KB");
        assert_eq!(fmt_bytes(1536), "1.5 KB");
        assert_eq!(fmt_bytes(1048576), "1.0 MB");
        assert_eq!(fmt_bytes(1073741824), "1.0 GB");
        assert_eq!(fmt_bytes(1099511627776), "1.0 TB");
    }

    #[test]
    fn flat_mode_no_traffic() {
        let input = "\
===IFACE eth0
qdisc netem 1: root refcnt 2 limit 1000 delay 80ms 20ms loss 2%
 Sent 0 bytes 0 pkt (dropped 0, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
---FILTERS---
";
        let output = digest(input);
        assert_eq!(output, "eth0: no traffic\n");
    }

    #[test]
    fn flat_mode_with_traffic() {
        let input = "\
===IFACE eth0
qdisc netem 1: root refcnt 2 limit 1000 delay 80ms 20ms loss 2%
 Sent 15360 bytes 120 pkt (dropped 3, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
---FILTERS---
";
        let output = digest(input);
        assert_eq!(
            output,
            "\
eth0:
  impairment: delay 80ms 20ms loss 2%
  traffic:    15.0 KB sent (120 packets), 3 dropped
"
        );
    }

    #[test]
    fn flat_mode_no_impairment() {
        let input = "\
===IFACE eth0
qdisc netem 1: root refcnt 2 limit 1000
 Sent 1024 bytes 10 pkt (dropped 0, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
---FILTERS---
";
        let output = digest(input);
        assert_eq!(
            output,
            "\
eth0:
  impairment: (no impairment)
  traffic:    1.0 KB sent (10 packets), 0 dropped
"
        );
    }

    #[test]
    fn per_link_mode() {
        let input = "\
===IFACE eth0
qdisc htb 1: root refcnt 2 r2q 10 default 0x30 direct_packets_stat 0 direct_qlen 1000
 Sent 0 bytes 0 pkt (dropped 0, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
qdisc netem 10: parent 1:10 limit 1000 delay 200ms 50ms loss 5%
 Sent 51200 bytes 400 pkt (dropped 20, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
qdisc netem 20: parent 1:20 limit 1000 delay 10ms 2ms
 Sent 25600 bytes 200 pkt (dropped 0, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
qdisc netem 30: parent 1:30 limit 1000 delay 80ms 20ms loss 2%
 Sent 1024 bytes 8 pkt (dropped 0, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
---FILTERS---
filter parent 1: protocol ip pref 1 u32 chain 0 fh 800::800 order 2048 key ht 0x800 bkt 0x0 flowid 1:10 not_in_hw
  match 0a63000a/ffffffff at 16
filter parent 1: protocol ip pref 1 u32 chain 0 fh 800::801 order 2049 key ht 0x800 bkt 0x0 flowid 1:20 not_in_hw
  match 0a630014/ffffffff at 16
";
        let output = digest(input);
        assert_eq!(
            output,
            "\
eth0: per-link
  link 1:10 -> 10.99.0.10
    impairment: delay 200ms 50ms loss 5%
    traffic:    50.0 KB sent (400 packets), 20 dropped
  link 1:20 -> 10.99.0.20
    impairment: delay 10ms 2ms
    traffic:    25.0 KB sent (200 packets), 0 dropped
  default (1:30)
    impairment: delay 80ms 20ms loss 2%
    traffic:    1.0 KB sent (8 packets), 0 dropped
"
        );
    }

    #[test]
    fn per_link_default_no_traffic() {
        let input = "\
===IFACE eth0
qdisc htb 1: root refcnt 2 r2q 10 default 0x30 direct_packets_stat 0 direct_qlen 1000
 Sent 0 bytes 0 pkt (dropped 0, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
qdisc netem 10: parent 1:10 limit 1000 delay 200ms 50ms loss 5%
 Sent 51200 bytes 400 pkt (dropped 20, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
qdisc netem 30: parent 1:30 limit 1000 delay 80ms 20ms loss 2%
 Sent 0 bytes 0 pkt (dropped 0, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
---FILTERS---
filter parent 1: protocol ip pref 1 u32 chain 0 fh 800::800 order 2048 key ht 0x800 bkt 0x0 flowid 1:10 not_in_hw
  match 0a63000a/ffffffff at 16
";
        let output = digest(input);
        // Default class has no traffic, so it should be omitted.
        assert_eq!(
            output,
            "\
eth0: per-link
  link 1:10 -> 10.99.0.10
    impairment: delay 200ms 50ms loss 5%
    traffic:    50.0 KB sent (400 packets), 20 dropped
"
        );
    }

    #[test]
    fn per_link_some_links_no_traffic() {
        let input = "\
===IFACE eth0
qdisc htb 1: root refcnt 2 r2q 10 default 0x30 direct_packets_stat 0 direct_qlen 1000
 Sent 0 bytes 0 pkt (dropped 0, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
qdisc netem 10: parent 1:10 limit 1000 delay 200ms 50ms loss 5%
 Sent 51200 bytes 400 pkt (dropped 20, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
qdisc netem 20: parent 1:20 limit 1000 delay 10ms 2ms
 Sent 0 bytes 0 pkt (dropped 0, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
qdisc netem 30: parent 1:30 limit 1000 delay 80ms 20ms loss 2%
 Sent 0 bytes 0 pkt (dropped 0, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
---FILTERS---
filter parent 1: protocol ip pref 1 u32 chain 0 fh 800::800 order 2048 key ht 0x800 bkt 0x0 flowid 1:10 not_in_hw
  match 0a63000a/ffffffff at 16
filter parent 1: protocol ip pref 1 u32 chain 0 fh 800::801 order 2049 key ht 0x800 bkt 0x0 flowid 1:20 not_in_hw
  match 0a630014/ffffffff at 16
";
        let output = digest(input);
        // Link 20 and default have no traffic, so only link 10 is shown.
        assert_eq!(
            output,
            "\
eth0: per-link
  link 1:10 -> 10.99.0.10
    impairment: delay 200ms 50ms loss 5%
    traffic:    50.0 KB sent (400 packets), 20 dropped
"
        );
    }

    #[test]
    fn non_netem_interface_ignored() {
        let input = "\
===IFACE lo
qdisc noqueue 0: root refcnt 2
 Sent 0 bytes 0 pkt (dropped 0, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
---FILTERS---
===IFACE eth0
qdisc netem 1: root refcnt 2 limit 1000 delay 80ms 20ms
 Sent 2048 bytes 16 pkt (dropped 1, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
---FILTERS---
";
        let output = digest(input);
        // lo has no netem, so only eth0 appears.
        assert_eq!(
            output,
            "\
eth0:
  impairment: delay 80ms 20ms
  traffic:    2.0 KB sent (16 packets), 1 dropped
"
        );
    }

    #[test]
    fn multiple_interfaces() {
        let input = "\
===IFACE eth0
qdisc netem 1: root refcnt 2 limit 1000 delay 100ms
 Sent 4096 bytes 32 pkt (dropped 2, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
---FILTERS---
===IFACE eth1
qdisc netem 1: root refcnt 2 limit 1000 delay 50ms
 Sent 0 bytes 0 pkt (dropped 0, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
---FILTERS---
";
        let output = digest(input);
        assert_eq!(
            output,
            "\
eth0:
  impairment: delay 100ms
  traffic:    4.0 KB sent (32 packets), 2 dropped
eth1: no traffic
"
        );
    }
}
