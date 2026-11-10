use regex::{Regex, bytes::Regex as BytesRegex};
use std::{borrow::Cow, sync::LazyLock};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

static SVLOGD: LazyLock<BytesRegex> = LazyLock::new(|| {
    BytesRegex::new(r"^[0-9]+-[0-9]+-[0-9]+_[0-9]+ [0-9]+:[0-9]+\.[0-9]+ ").unwrap()
});
static NAMESPACE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\[ [ \t\n\x0B\x0C\r]*([^ ]+) [ \t\n\x0B\x0C\r]*\] [^$]+").unwrap()
});
static PID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([^\[]+)\[([0-9]+)\]$").unwrap());

#[derive(Debug)]
pub struct Message<'a> {
    pub host: &'a str,
    pub program: &'a str,
    pub body: Cow<'a, str>,
}
impl Message<'_> {
    pub fn namespace(&self) -> &str {
        NAMESPACE
            .captures(&self.body)
            .and_then(|c| c.get(1))
            .map_or("", |m| m.as_str())
    }
}
fn field(text: &[u8]) -> Option<(&[u8], &[u8])> {
    let space = text.iter().position(|byte| *byte == b' ')?;
    (space != 0).then_some((&text[..space], &text[space + 1..]))
}
fn header(text: &[u8]) -> Result<&str, &'static str> {
    std::str::from_utf8(text).map_err(|_| "utf8")
}
pub fn parse(line: &[u8]) -> Result<Message<'_>, &'static str> {
    let (date, rest) = field(line).ok_or("header")?;
    OffsetDateTime::parse(header(date)?, &Rfc3339).map_err(|_| "timestamp")?;
    let (host, rest) = field(rest).ok_or("header")?;
    let mut host = header(host)?;
    let (program, body) = if host == "localhost" && SVLOGD.is_match(rest) {
        let (_, rest) = field(rest).ok_or("header")?;
        let (_, rest) = field(rest).ok_or("header")?;
        let (inner_host, rest) = field(rest).ok_or("header")?;
        host = header(inner_host)?;
        // The forwarded program remains verbatim, except for safe kernel normalization.
        let (program, body) = field(rest).ok_or("header")?;
        (header(program)?, body)
    } else {
        let (program, body) = field(rest).ok_or("header")?;
        let program = header(program)?;
        let program = PID
            .captures(program)
            .and_then(|c| c.get(1))
            .map_or(program, |m| m.as_str());
        (program, body)
    };
    let mut program = program;
    if program == "kernel[]"
        || program
            .strip_prefix("kernel[")
            .and_then(|p| p.strip_suffix(']'))
            .is_some_and(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()))
    {
        program = "kernel";
    }
    Ok(Message {
        host,
        program,
        body: String::from_utf8_lossy(body),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_shapes() {
        let m = parse(b"2026-07-09T12:00:00+00:00 host osctld[123] Received command 'ct.start'\n")
            .unwrap();
        assert_eq!(
            (m.host, m.program, m.body.as_ref()),
            ("host", "osctld", "Received command 'ct.start'\n")
        );
        for program in ["kernel", "kernel[]", "kernel[123]"] {
            let line = format!("2026-07-09T12:00:00.123456+02:00 host {program} [ prg ] text\n");
            let m = parse(line.as_bytes()).unwrap();
            assert_eq!((m.program, m.namespace()), ("kernel", "prg"));
        }
        let m = parse(
            b"2026-07-09T12:00:00Z localhost 2026-07-09_12 00:00.000000 host kernel[] text\n",
        )
        .unwrap();
        assert_eq!((m.host, m.program, m.namespace()), ("host", "kernel", ""));
        assert_eq!(parse(b"garbage").unwrap_err(), "header");
        assert_eq!(parse(b"bad host x body\n").unwrap_err(), "timestamp");
        assert_eq!(
            parse(b"2026-07-09T12:00:00Z host osctld: body\n")
                .unwrap()
                .program,
            "osctld:"
        );
    }

    #[test]
    fn compatibility_body_borrowing_and_replacement() {
        for body in ["normal\n", "ž \u{fffd}\n", " [ ns ] text\n"] {
            let line = format!("2026-11-11T12:00:00Z host x {body}");
            let message = parse(line.as_bytes()).unwrap();
            assert!(matches!(&message.body, Cow::Borrowed(_)));
            assert_eq!(message.body, body);
        }
        for (body, expected) in [
            (
                b"\xffDaemon crashed\n".as_slice(),
                "\u{fffd}Daemon crashed\n",
            ),
            (
                b"Daemon crashed \xff\n".as_slice(),
                "Daemon crashed \u{fffd}\n",
            ),
            (
                b"Daemon cra\xffshed\n".as_slice(),
                "Daemon cra\u{fffd}shed\n",
            ),
            (
                b"Daemon crashed \xe2\x82\n".as_slice(),
                "Daemon crashed \u{fffd}\n",
            ),
        ] {
            let mut line = b"2026-11-11T12:00:00Z host nodectld ".to_vec();
            line.extend_from_slice(body);
            let message = parse(&line).unwrap();
            assert!(matches!(&message.body, Cow::Owned(_)));
            assert_eq!(message.body, expected);
        }
    }

    #[test]
    fn compatibility_headers_remain_strict_and_forwarding_is_ascii() {
        for line in [
            b"2026-\xff host x body\n".as_slice(),
            b"2026-11-11T12:00:00Z ho\xffst x body\n",
            b"2026-11-11T12:00:00Z host x\xff body\n",
            b"2026-11-11T12:00:00Z localhost 2026-11-11_12 00:00.000 ho\xffst x body\n",
            b"2026-11-11T12:00:00Z localhost 2026-11-11_12 00:00.000 host x\xff body\n",
        ] {
            assert_eq!(parse(line).unwrap_err(), "utf8");
        }
        let message = parse(b"2026-11-11T12:00:00Z localhost 2026-11-11_12 00:00.000 host nodectld Daemon crashed \xff\n").unwrap();
        assert_eq!((message.host, message.program), ("host", "nodectld"));
        assert_eq!(message.body, "Daemon crashed \u{fffd}\n");
        let message = parse(
            b"2026-11-11T12:00:00Z localhost x 2026-11-11_12 00:00.000 ho\xffst nodectld body\n",
        )
        .unwrap();
        assert_eq!((message.host, message.program), ("localhost", "x"));
        assert!(message.body.contains("ho\u{fffd}st"));
        for prefix in ["2026-11-11_١ 00:00.000", "2026-11-11_12 00:١٠.000"] {
            let line =
                format!("2026-11-11T12:00:00Z localhost {prefix} host nodectld Daemon crashed\n");
            let message = parse(line.as_bytes()).unwrap();
            assert_eq!(message.host, "localhost");
            assert_eq!(message.program, prefix.split_once(' ').unwrap().0);
        }
        for (program, expected) in [("kernel[123]", "kernel"), ("kernel[١]", "kernel[١]")] {
            let line = format!("2026-11-11T12:00:00Z host {program} body\n");
            assert_eq!(parse(line.as_bytes()).unwrap().program, expected);
        }
    }
}
