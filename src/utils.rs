use std::{env, str::FromStr};

use ropey::Rope;
use tower_lsp::lsp_types::*;

use crate::pkg;
use crate::styles;
use crate::vale;

pub(crate) fn make_title(action: String, matched: String, fix: String) -> String {
    match action.as_str() {
        "remove" => format!("Remove ‘{}’", matched),
        _ => format!("Replace with ‘{}’", fix),
    }
}

pub(crate) fn vale_arch() -> String {
    let platform = match env::consts::OS {
        "windows" => "Windows",
        "macos" => "macOS",
        _ => "Linux",
    };
    let arch = match env::consts::ARCH {
        "x86_64" => "64-bit",
        "arm" => "arm64",
        "aarch64" => "arm64",
        _ => "386",
    };
    format!("{}_{}", platform, arch)
}

pub(crate) fn position_to_range(p: Position, rope: &Rope) -> Option<Range> {
    let line = p.line as usize;
    let index = p.character as usize;

    let context = rope.line(line);
    let extent = context.chars().count() - 1;

    let mut start = index;
    while start > 0 && !context.char(start - 1).is_whitespace() {
        start -= 1;
    }

    let mut end = index;
    while end < extent && !context.char(end + 1).is_whitespace() {
        end += 1;
    }

    if start == end {
        return None;
    } else if end > index {
        // TODO: Why is this necessary?
        //
        // FIXME:
        //
        // BasedOnStyles = Vale
        //                   ^
        end += 1;
    }

    Some(Range::new(
        Position::new(line as u32, start as u32),
        Position::new(line as u32, end as u32),
    ))
}

pub(crate) fn range_to_token(r: Range, rope: &Rope) -> String {
    let start = r.start.character as usize;
    let end = r.end.character as usize;

    let context = rope.line(r.start.line as usize);
    let token = context.slice(start..end).as_str().unwrap_or("");

    token.to_string()
}

pub(crate) fn alert_to_range(alert: vale::ValeAlert) -> Range {
    Range {
        start: Position {
            line: alert.line as u32 - 1,
            character: alert.span.0 as u32 - 1,
        },
        end: Position {
            line: alert.line as u32 - 1,
            character: alert.span.1 as u32,
        },
    }
}

pub(crate) fn severity_to_level(severity: String) -> DiagnosticSeverity {
    match severity.as_str() {
        "error" => DiagnosticSeverity::ERROR,
        "warning" => DiagnosticSeverity::WARNING,
        "suggestion" => DiagnosticSeverity::INFORMATION,
        _ => DiagnosticSeverity::HINT,
    }
}

pub(crate) fn entry_to_completion(v: styles::PathEntry) -> CompletionItem {
    CompletionItem {
        label: v.name.clone(),
        insert_text: Some(v.name.clone()),
        kind: Some(CompletionItemKind::VALUE),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: v.path.display().to_string(),
        })),
        detail: Some(v.kind.to_string()),
        ..CompletionItem::default()
    }
}

pub(crate) fn pkg_to_completion(pkg: pkg::Package) -> CompletionItem {
    CompletionItem {
        label: pkg.name.clone(),
        insert_text: Some(pkg.name.clone()),
        kind: Some(CompletionItemKind::VALUE),
        label_details: Some(CompletionItemLabelDetails {
            description: Some(pkg.description),
            ..CompletionItemLabelDetails::default()
        }),
        detail: Some("Package".to_string()),
        preselect: Some(true),
        ..CompletionItem::default()
    }
}

pub(crate) fn alert_to_diagnostic(alert: &vale::ValeAlert) -> Diagnostic {
    let mut d = Diagnostic {
        range: alert_to_range(alert.clone()),
        severity: Some(severity_to_level(alert.severity.clone())),
        code: Some(NumberOrString::String(alert.check.clone())),
        source: Some("vale-ls".to_string()),
        message: alert.message.clone(),
        related_information: None,
        code_description: None,
        tags: None,
        data: Some(serde_json::to_value(alert).unwrap()),
    };

    if alert.link != "" {
        let uri = Url::from_str(&alert.link);
        if uri.is_ok() {
            d.code_description = Some(CodeDescription {
                href: Some(uri.unwrap()).unwrap(),
            });
        }
    }

    d
}

/// `error_to_diagnostic` places an error Vale reported in a file -- a
/// `.vale.ini` or a rule, as an E201 is -- as a diagnostic on that file, from
/// its column to the end of the line. An error with no position, such as an
/// E100, has nowhere to go and returns `None`.
pub(crate) fn error_to_diagnostic(err: &vale::ValeError) -> Option<(Url, Diagnostic)> {
    if err.path.is_empty() || err.line == 0 {
        return None;
    }
    let uri = Url::from_file_path(&err.path).ok()?;

    let line = err.line - 1;
    let start = err.span.saturating_sub(1);
    let end = std::fs::read_to_string(&err.path)
        .ok()
        .and_then(|src| {
            src.lines()
                .nth(line as usize)
                .map(|l| l.chars().count() as u32)
        })
        .filter(|&len| len > start)
        .unwrap_or(start + 1);

    let code = if err.code.is_empty() {
        "E201"
    } else {
        &err.code
    };
    Some((
        uri,
        Diagnostic {
            range: Range {
                start: Position {
                    line,
                    character: start,
                },
                end: Position {
                    line,
                    character: end,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String(code.to_string())),
            source: Some("vale-ls".to_string()),
            message: err.text.clone(),
            ..Diagnostic::default()
        },
    ))
}

/// `metrics_summary` condenses Vale's metrics into a one-line code lens.
pub(crate) fn metrics_summary(metrics: &serde_json::Map<String, serde_json::Value>) -> String {
    let count = |key: &str| metrics.get(key).and_then(|v| v.as_i64());

    let parts: Vec<String> = [("words", "word"), ("sentences", "sentence")]
        .iter()
        .filter_map(|(key, noun)| {
            let n = count(key)?;
            Some(format!("{} {}{}", n, noun, if n == 1 { "" } else { "s" }))
        })
        .collect();

    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_land_on_their_line() {
        let dir = tempfile::tempdir().unwrap();
        let ini = dir.path().join(".vale.ini");
        std::fs::write(
            &ini,
            "StylesPath = styles\n\n[*.md]\nBasedOnStyles = Vale, Nope\n",
        )
        .unwrap();

        let err = vale::ValeError {
            path: ini.display().to_string(),
            text: "Style 'Nope' isn't on the StylesPath.".to_string(),
            line: 4,
            span: 23,
            code: "E201".to_string(),
        };
        let (uri, d) = error_to_diagnostic(&err).unwrap();
        assert_eq!(uri, Url::from_file_path(&ini).unwrap());
        assert_eq!((d.range.start.line, d.range.start.character), (3, 22));
        assert_eq!(d.range.end.character, 26);
        assert_eq!(d.code, Some(NumberOrString::String("E201".to_string())));

        let runtime = vale::ValeError {
            path: String::new(),
            text: "one argument expected".to_string(),
            line: 0,
            span: 0,
            code: "E100".to_string(),
        };
        assert!(error_to_diagnostic(&runtime).is_none());
    }

    #[test]
    fn summary() {
        let one: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"words": 1, "sentences": 1, "characters": 4}"#).unwrap();
        assert_eq!(metrics_summary(&one), "1 word, 1 sentence");

        let many: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"words": 12, "sentences": 3}"#).unwrap();
        assert_eq!(metrics_summary(&many), "12 words, 3 sentences");

        // Vale reported something we don't recognize.
        let empty: serde_json::Map<String, serde_json::Value> = serde_json::from_str("{}").unwrap();
        assert_eq!(metrics_summary(&empty), "");
    }

    #[test]
    fn arch() {
        let arch = vale_arch();
        match env::consts::OS {
            "windows" => assert_eq!(arch, "Windows_64-bit"),
            "macos" => assert!(arch == "macOS_64-bit" || arch == "macOS_arm64"),
            _ => assert_eq!(arch, "Linux_64-bit"),
        }
    }
}
