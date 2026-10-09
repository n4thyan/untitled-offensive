//! Shared administrator command registry used by the local console and RCON.

pub const MAX_COMMAND_BYTES: usize = 1024;
pub const MAX_OUTPUT_BYTES: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub usage: &'static str,
    pub summary: &'static str,
}

pub const COMMANDS: &[Spec] = &[
    Spec {
        name: "help",
        aliases: &["commands"],
        usage: "help [command]",
        summary: "list commands or show command help",
    },
    Spec {
        name: "status",
        aliases: &["players"],
        usage: "status",
        summary: "show server and player status",
    },
    Spec {
        name: "serverinfo",
        aliases: &[],
        usage: "serverinfo",
        summary: "show server settings",
    },
    Spec {
        name: "version",
        aliases: &[],
        usage: "version",
        summary: "show server version",
    },
    Spec {
        name: "uptime",
        aliases: &[],
        usage: "uptime",
        summary: "show server uptime",
    },
    Spec {
        name: "say",
        aliases: &[],
        usage: "say <message>",
        summary: "broadcast a chat message",
    },
    Spec {
        name: "tell",
        aliases: &[],
        usage: "tell <slot> <message>",
        summary: "send a message to one client",
    },
    Spec {
        name: "centerprint",
        aliases: &[],
        usage: "centerprint <message>",
        summary: "broadcast a center-print message",
    },
    Spec {
        name: "kick",
        aliases: &["clientkick"],
        usage: "kick <slot|name> [reason]",
        summary: "disconnect one client",
    },
    Spec {
        name: "kickall",
        aliases: &[],
        usage: "kickall [reason]",
        summary: "disconnect all human clients",
    },
    Spec {
        name: "map",
        aliases: &[],
        usage: "map <name>",
        summary: "load a map",
    },
    Spec {
        name: "map_restart",
        aliases: &[],
        usage: "map_restart",
        summary: "restart the current map",
    },
    Spec {
        name: "map_rotate",
        aliases: &[],
        usage: "map_rotate",
        summary: "advance map rotation",
    },
    Spec {
        name: "gametype",
        aliases: &[],
        usage: "gametype [name]",
        summary: "get or set g_gametype",
    },
    Spec {
        name: "mapvote",
        aliases: &[],
        usage: "mapvote",
        summary: "open a map vote",
    },
    Spec {
        name: "cancelvote",
        aliases: &[],
        usage: "cancelvote",
        summary: "cancel the current map vote",
    },
    Spec {
        name: "addbot",
        aliases: &[],
        usage: "addbot [count]",
        summary: "add debug bots",
    },
    Spec {
        name: "kickbot",
        aliases: &[],
        usage: "kickbot <slot|name>",
        summary: "remove one bot",
    },
    Spec {
        name: "kickbots",
        aliases: &[],
        usage: "kickbots",
        summary: "remove all bots",
    },
    Spec {
        name: "botcount",
        aliases: &[],
        usage: "botcount [count]",
        summary: "get or set desired bot count",
    },
    Spec {
        name: "botsshoot",
        aliases: &[],
        usage: "botsshoot [0|1]",
        summary: "get or set bot combat",
    },
    Spec {
        name: "set",
        aliases: &[],
        usage: "set <cvar> <value>",
        summary: "set a cvar",
    },
    Spec {
        name: "get",
        aliases: &[],
        usage: "get <cvar>",
        summary: "read a cvar",
    },
    Spec {
        name: "sv_hostname",
        aliases: &[],
        usage: "sv_hostname [value]",
        summary: "get or set server hostname",
    },
    Spec {
        name: "sv_maxclients",
        aliases: &[],
        usage: "sv_maxclients [value]",
        summary: "get or set client limit for the next level",
    },
    Spec {
        name: "g_password",
        aliases: &[],
        usage: "g_password [value]",
        summary: "get or set join password",
    },
    Spec {
        name: "pause",
        aliases: &[],
        usage: "pause",
        summary: "pause simulation",
    },
    Spec {
        name: "unpause",
        aliases: &[],
        usage: "unpause",
        summary: "resume simulation",
    },
    Spec {
        name: "dumpstats",
        aliases: &[],
        usage: "dumpstats",
        summary: "print bounded server statistics",
    },
    Spec {
        name: "dumpprofile",
        aliases: &[],
        usage: "dumpprofile [slot|name]",
        summary: "print a bounded class profile summary",
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed<'a> {
    pub spec: &'static Spec,
    pub invoked_as: &'a str,
    pub args: &'a str,
}

pub fn find(name: &str) -> Option<&'static Spec> {
    COMMANDS.iter().find(|spec| {
        spec.name.eq_ignore_ascii_case(name)
            || spec
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(name))
    })
}

pub fn parse(line: &str) -> Result<Parsed<'_>, &'static str> {
    if line.len() > MAX_COMMAND_BYTES || line.contains(['\0', '\r', '\n']) {
        return Err("invalid or oversized command");
    }
    let line = line.trim();
    let (word, args) = line
        .split_once(char::is_whitespace)
        .map_or((line, ""), |(word, args)| (word, args.trim()));
    if word.is_empty() {
        return Err("empty command");
    }
    let spec = find(word).ok_or("unknown command")?;
    Ok(Parsed {
        spec,
        invoked_as: word,
        args,
    })
}

pub fn command_list() -> String {
    COMMANDS
        .iter()
        .flat_map(|spec| std::iter::once(spec.name).chain(spec.aliases.iter().copied()))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn bounded(mut text: String) -> String {
    if text.len() <= MAX_OUTPUT_BYTES {
        return text;
    }
    let mut end = MAX_OUTPUT_BYTES.saturating_sub("\n[output truncated]".len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text.push_str("\n[output truncated]");
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_resolve_through_the_same_registry() {
        assert_eq!(parse("commands").unwrap().spec.name, "help");
        assert_eq!(parse("clientkick 2").unwrap().spec.name, "kick");
        assert_eq!(parse("players").unwrap().spec.name, "status");
    }

    #[test]
    fn parser_rejects_embedded_line_breaks_and_oversize() {
        assert!(parse("status\nmap mp_ship").is_err());
        assert!(parse(&"x".repeat(MAX_COMMAND_BYTES + 1)).is_err());
    }

    #[test]
    fn output_truncation_is_utf8_safe_and_bounded() {
        let output = bounded("£".repeat(MAX_OUTPUT_BYTES));
        assert!(output.len() <= MAX_OUTPUT_BYTES);
        assert!(output.ends_with("[output truncated]"));
    }
}
