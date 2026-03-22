use anyhow::{bail, Result};
use std::io::Write;
use std::process::{Command, Stdio};
use crate::notification::Action;

/// Presents `actions` to the user via the configured external picker command.
/// Returns the `Action` the user selected, or an error if picker exits non-zero or no match.
pub fn pick_action(picker_cmd: &str, actions: &[Action]) -> Result<Action> {
    if actions.is_empty() {
        bail!("last notification has no actions");
    }

    // Parse the picker command into program + args
    let mut parts = shell_words(picker_cmd);
    if parts.is_empty() {
        bail!("picker command is empty");
    }
    let program = parts.remove(0);

    let mut child = Command::new(&program)
        .args(&parts)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to launch picker '{}': {}", program, e))?;

    // Write labels to stdin
    {
        let stdin = child.stdin.as_mut().unwrap();
        for action in actions {
            writeln!(stdin, "{}", action.label)?;
        }
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!("picker cancelled (exit code {})", output.status);
    }

    let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
    actions
        .iter()
        .find(|a| a.label == selected)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("picker returned unknown label: '{}'", selected))
}

/// Minimal shell word splitter: splits on spaces, respects single and double quotes.
fn shell_words(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    for ch in s.chars() {
        match ch {
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            ' ' if !in_single_quote && !in_double_quote => {
                if !current.is_empty() {
                    words.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() { words.push(current); }
    words
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notification::Action;

    fn make_actions(pairs: &[(&str, &str)]) -> Vec<Action> {
        pairs.iter().map(|(k, l)| Action { key: k.to_string(), label: l.to_string() }).collect()
    }

    #[test]
    fn test_pick_action_empty_actions_errors() {
        let result = pick_action("cat", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no actions"));
    }

    #[test]
    fn test_pick_action_selects_correct_action() {
        // Use `echo` as the picker — it ignores stdin and outputs its argument
        let actions = make_actions(&[("default", "Open"), ("snooze", "Snooze")]);
        let result = pick_action("echo Open", &actions);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().key, "default");
    }

    #[test]
    fn test_pick_action_non_zero_exit_is_cancel() {
        // `false` always exits with code 1
        let actions = make_actions(&[("default", "Open")]);
        let result = pick_action("false", &actions);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }

    #[test]
    fn test_shell_words_simple() {
        assert_eq!(shell_words("rofi -dmenu"), vec!["rofi", "-dmenu"]);
    }

    #[test]
    fn test_shell_words_single_quoted() {
        assert_eq!(
            shell_words("rofi -dmenu -p 'Action:'"),
            vec!["rofi", "-dmenu", "-p", "Action:"]
        );
    }

    #[test]
    fn test_shell_words_empty() {
        assert!(shell_words("").is_empty());
    }

    #[test]
    fn test_shell_words_double_quoted() {
        assert_eq!(
            shell_words(r#"wofi --prompt "Select action:""#),
            vec!["wofi", "--prompt", "Select action:"]
        );
    }

    #[test]
    fn test_shell_words_double_quoted_with_spaces() {
        assert_eq!(
            shell_words(r#"rofi -dmenu -p "Pick an action:""#),
            vec!["rofi", "-dmenu", "-p", "Pick an action:"]
        );
    }
}
