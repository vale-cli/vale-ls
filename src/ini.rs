use std::collections::HashMap;
use std::path::PathBuf;

use regex::Regex;
use tower_lsp::lsp_types::*;

use crate::error::Error;
use crate::pkg;
use crate::styles::StylesPath;
use crate::utils;

pub fn key_to_info(key: &str) -> Option<&str> {
    // A section header is a token in its own right -- `[formats]` -- so the
    // brackets come along with it.
    match key.trim_matches(|c| c == '[' || c == ']') {
        "formats" => Some(include_str!("../doc/ini/section/Formats.md")),
        "asciidoctor" => Some(include_str!("../doc/ini/section/Asciidoctor.md")),
        "StylesPath" => Some(include_str!("../doc/ini/StylesPath.md")),
        "MinAlertLevel" => Some(include_str!("../doc/ini/MinAlertLevel.md")),
        "IgnoredScopes" => Some(include_str!("../doc/ini/IgnoredScopes.md")),
        "IgnoredClasses" => Some(include_str!("../doc/ini/IgnoredClasses.md")),
        "SkippedScopes" => Some(include_str!("../doc/ini/SkippedScopes.md")),
        "WordTemplate" => Some(include_str!("../doc/ini/WordTemplate.md")),
        "BasedOnStyles" => Some(include_str!("../doc/ini/BasedOnStyles.md")),
        "BlockIgnores" => Some(include_str!("../doc/ini/BlockIgnores.md")),
        "TokenIgnores" => Some(include_str!("../doc/ini/TokenIgnores.md")),
        "Transform" => Some(include_str!("../doc/ini/Transform.md")),
        "Vocab" => Some(include_str!("../doc/ini/Vocab.md")),
        "Packages" => Some(include_str!("../doc/ini/Packages.md")),
        "CommentDelimiters" => Some(include_str!("../doc/ini/CommentDelimiters.md")),
        "IgnorePatterns" => Some(include_str!("../doc/ini/IgnorePatterns.md")),
        "Lang" => Some(include_str!("../doc/ini/Lang.md")),
        "NLPEndpoint" => Some(include_str!("../doc/ini/NLPEndpoint.md")),
        "View" => Some(include_str!("../doc/ini/View.md")),
        _ => None,
    }
}

pub async fn complete(line: &str, styles: Vec<PathBuf>) -> Result<Vec<CompletionItem>, Error> {
    let mut completions = Vec::new();
    let re = Regex::new(r"\w+\.\w+ =").unwrap();

    if line.contains("BasedOnStyles") {
        completions = get_styles(line, styles)?;
    } else if line.contains("MinAlertLevel") {
        vec!["suggestion", "warning", "error"]
            .into_iter()
            .for_each(|s| {
                completions.push(CompletionItem {
                    label: s.to_string(),
                    kind: Some(CompletionItemKind::VALUE),
                    ..CompletionItem::default()
                })
            });
    } else if line.contains("IgnoredScopes") {
        completions = inline_tags();
    } else if line.contains("SkippedScopes") {
        completions = block_tags();
    } else if re.is_match(line) {
        completions = rule_options();
    } else if line.contains("Vocab") {
        completions = get_vocab(line, styles)?;
    } else if line.contains("Packages") {
        completions = get_pkgs(line).await?;
    }

    Ok(completions)
}

async fn get_pkgs(line: &str) -> Result<Vec<CompletionItem>, Error> {
    let pkgs: Vec<pkg::Package> = pkg::fetch().await?;

    let completions = pkgs
        .into_iter()
        .filter(|v| !line.contains(&v.name))
        .map(|v| utils::pkg_to_completion(v))
        .collect();

    Ok(completions)
}

fn get_vocab(line: &str, styles: Vec<PathBuf>) -> Result<Vec<CompletionItem>, Error> {
    let p = StylesPath::new(styles);

    let completions = p
        .get_vocab()?
        .into_iter()
        .filter(|v| !line.contains(&v.name))
        .map(|v| utils::entry_to_completion(v))
        .collect();

    Ok(completions)
}

fn get_styles(line: &str, styles: Vec<PathBuf>) -> Result<Vec<CompletionItem>, Error> {
    let p = StylesPath::new(styles);

    let completions = p
        .get_styles()?
        .into_iter()
        .filter(|v| !line.contains(&v.name))
        .map(|v| utils::entry_to_completion(v))
        .collect();

    Ok(completions)
}

fn rule_options() -> Vec<CompletionItem> {
    let mut completions = Vec::new();

    let options = HashMap::from([
        ("YES", "Enable the given rule in this scope."),
        ("NO", "Disable the given rule in this scope."),
        ("suggestion", "Set the severity to 'suggestion'."),
        ("warning", "Set the severity to 'warning'."),
        ("error", "Set the severity to 'error'."),
    ]);

    for (key, value) in options {
        completions.push(CompletionItem {
            label: key.to_string(),
            kind: Some(CompletionItemKind::VALUE),
            label_details: Some(CompletionItemLabelDetails {
                description: Some(format!("{}", value)),
                ..CompletionItemLabelDetails::default()
            }),
            ..CompletionItem::default()
        });
    }

    completions
}

fn inline_tags() -> Vec<CompletionItem> {
    vec!["small", "abbr", "em", "kbd", "tt", "code", "b", "i"]
        .into_iter()
        .map(|s| CompletionItem {
            label: s.to_string(),
            kind: Some(CompletionItemKind::VALUE),
            ..CompletionItem::default()
        })
        .collect()
}

fn block_tags() -> Vec<CompletionItem> {
    vec!["script", "style", "pre", "figure"]
        .into_iter()
        .map(|s| CompletionItem {
            label: s.to_string(),
            kind: Some(CompletionItemKind::VALUE),
            ..CompletionItem::default()
        })
        .collect()
}
